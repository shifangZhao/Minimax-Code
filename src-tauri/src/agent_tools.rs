// Agent Tools — standalone tool implementations for AI agent operations.
//
// All tool functions are pub(crate) and called from agent_service::execute_tool.

use base64::Engine as _;
use rusqlite::Connection;
use serde_json::json;
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::lsp_types::format_diagnostics;
use crate::{DEFAULT_API_URL, SEARCH_TIMEOUT_SECS, VLM_TIMEOUT_SECS};

use crate::agent_service::{PendingAsks, hidden_cmd, output_with_timeout};
use crate::lsp_manager::LspManager;
use crate::mcp_service::McpService;
use crate::skill_service::SkillService;

// ── Tool output limits ──
const TOOL_OUTPUT_MAX_BYTES: usize = 50 * 1024; // 50 KB
const TOOL_OUTPUT_MAX_LINES: usize = 2000;
const TOOL_OUTPUT_TAIL_FRACTION: f64 = 0.15;

// ── File read limits ──
const FILE_READ_MAX_SIZE: u64 = 2 * 1024 * 1024; // 2 MB
const FILE_OUTLINE_THRESHOLD: u64 = 64 * 1024; // 64 KB
const FILE_OUTLINE_PREVIEW_LINES: usize = 300;
const FILE_DEFAULT_READ_LIMIT: usize = 200;
const BINARY_DETECT_BUFFER_SIZE: usize = 8192;
const MULTIFILE_MAX_LINES: usize = 500;
const MULTIFILE_MAX_BYTES: usize = 100 * 1024; // 100 KB

// ── Search limits ──
const SEARCH_MAX_RESULTS: usize = 200;
const SEARCH_SHOW_LIMIT: usize = 50;
const SEARCH_RECURSE_DEPTH: usize = 10;
const GLOB_DEFAULT_LIMIT: usize = 200;
const FILE_SEARCH_RESULT_LIMIT: usize = 100;

// ── Web fetch limits ──
const WEB_FETCH_MAX_CHARS: usize = 32_000;

// ── Process limits ──
const PROCESS_LIST_MAX_LINES: usize = 50;

// ── Vision API ──
const VISION_MAX_TOKENS: u64 = 2048;
const VISION_HTTP_TIMEOUT_SECS: u64 = 60;
const IMAGE_MAX_SIZE_BYTES: u64 = 50 * 1024 * 1024; // 50 MB

// ── Timeouts ──
const COMMAND_TIMEOUT_SECS: u64 = 120;
const ASK_CHOICE_TIMEOUT_SECS: u64 = 600;

/// Truncate tool output to prevent context explosion.
/// Caps at 50KB or 2000 lines, preserves head+tail, injects marker.
pub(crate) fn truncate_tool_result(output: String) -> String {
    const MAX_BYTES: usize = TOOL_OUTPUT_MAX_BYTES;
    const MAX_LINES: usize = TOOL_OUTPUT_MAX_LINES;
    const TAIL_FRACTION: f64 = TOOL_OUTPUT_TAIL_FRACTION;

    let bytes = output.len();
    let lines: Vec<&str> = output.lines().collect();
    let line_count = lines.len();

    if bytes <= MAX_BYTES && line_count <= MAX_LINES {
        return output;
    }

    let head_lines = ((MAX_LINES as f64) * (1.0 - TAIL_FRACTION)) as usize;
    let tail_lines = (MAX_LINES as f64 * TAIL_FRACTION) as usize;

    let head: String = lines.iter().take(head_lines).copied().collect::<Vec<_>>().join("\n");
    let tail: String = lines.iter().rev().take(tail_lines).rev().copied().collect::<Vec<_>>().join("\n");
    let skipped = bytes.saturating_sub(head.len() + tail.len());

    format!(
        "{}\n[...truncated {} bytes / {} lines ...]\n{}",
        head, skipped, line_count.saturating_sub(head_lines + tail_lines), tail
    )
}

/// Parse a command string into (program, args) respecting shell-style quoting.
/// The LLM is free to choose the shell (cmd /c ..., powershell -Command ..., sh -c ...)
/// or just pass raw commands like `git status`.
fn parse_command(command: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = command.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
        i += 1;
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.is_empty() {
        return (String::new(), vec![]);
    }
    let program = parts.remove(0);
    (program, parts)
}

/// Check whether a command string contains unquoted shell metacharacters
/// (pipes, redirects, chaining). These MUST be interpreted by a shell —
/// direct CreateProcess/execvp will pass them as arguments, producing
/// silently wrong output instead of an error.
fn has_shell_metacharacters(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            // Unambiguous shell operators — only significant outside quotes
            '|' | '>' | '<' if !in_single && !in_double => return true,
            '&' if !in_single && !in_double && i + 1 < len && chars[i + 1] == '&' => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Returns true if the program name is a known shell executable.
/// When the LLM explicitly chooses a shell, we execute directly without wrapping.
fn is_shell_program(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let lower = name.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    matches!(stem, "cmd" | "powershell" | "pwsh" | "sh" | "bash" | "zsh" | "fish" | "wsl" | "dash")
}

/// Execute a command through the system default shell.
/// Windows: `cmd /d /s /c` — /d skips autorun, /s handles quote stripping.
/// Unix: `sh -c`.
fn execute_via_shell(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
    running_pids: Option<&Arc<StdMutex<HashMap<i64, Vec<u32>>>>>,
) -> String {
    let mut cmd = hidden_cmd(if cfg!(windows) { "cmd" } else { "sh" });
    if cfg!(windows) {
        cmd.args(["/d", "/s", "/c", command]);
    } else {
        cmd.args(["-c", command]);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    output_with_timeout(&mut cmd, timeout_secs, session_id, running_pids)
}

// ========== Tool Implementations (Flat) ==========

pub(crate) async fn tool_list_dir(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut dirs: Vec<String> = Vec::new();
                let mut files: Vec<(String, u64, String, usize)> = Vec::new(); // (name, size, modified, lines)
                for e in entries.filter_map(|e| e.ok()) {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        dirs.push(format!("[DIR]  {}", name));
                    } else {
                        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                        let modified = e.metadata()
                            .and_then(|m| m.modified())
                            .map(|t| {
                                let dur = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                                let secs = dur.as_secs();
                                // Convert to Y-M-D H:M
                                let days = secs / 86400;
                                let (y, m, d) = days_to_ymd(days);
                                let time_of_day = secs % 86400;
                                let h = time_of_day / 3600;
                                let min = (time_of_day % 3600) / 60;
                                format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, h, min)
                            })
                            .unwrap_or_else(|_| "?".to_string());
                        let line_count = if size < 2_000_000 {
                            std::fs::read_to_string(e.path())
                                .map(|c| c.lines().count())
                                .unwrap_or(0)
                        } else { 0 };
                        files.push((name, size, modified, line_count));
                    }
                }
                dirs.sort();
                files.sort_by(|a, b| a.0.cmp(&b.0));
                let mut result = dirs;
                for (name, size, modified, lines) in files {
                    let size_str = format_size(size);
                    if lines > 0 {
                        result.push(format!("[FILE] {:>10}  {:>6} lines  {}  {}", size_str, lines, modified, name));
                    } else {
                        result.push(format!("[FILE] {:>10}  {:>13}  {}  {}", size_str, "", modified, name));
                    }
                }
                result.join("\n")
            }
            Err(e) => format!("Error: Cannot read directory '{}'", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / 1048576.0) }
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Simplified epoch-to-ymd (good enough for file timestamps)
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days: [u64; 12] = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut m = 1u64;
    for &md in &month_days {
        if remaining < md { break; }
        remaining -= md;
        m += 1;
    }
    (y, m, remaining + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

pub(crate) async fn tool_read_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let offset = params["offset"].as_u64().unwrap_or(0) as usize;
    let limit = params["limit"].as_u64().unwrap_or(0) as usize;

    tokio::task::spawn_blocking(move || {
        const MAX_SIZE: u64 = FILE_READ_MAX_SIZE;
        const OUTLINE_SIZE: u64 = FILE_OUTLINE_THRESHOLD;
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };
        // Binary detection
        if is_binary_file(&path) {
            return format!("Binary file: {} ({} KB). Use a hex editor or specialized tool.", path, meta.len() / 1024);
        }
        // Large file: outline mode
        if meta.len() > MAX_SIZE && offset == 0 && limit == 0 {
            let line_count = count_lines(&path);
            return format!(
                "文件过大: {} MB, {} 行，超过 {} MB 限制，内容已截断。\n\
                 使用 offset/limit 参数分段读取其余部分。\n\
                 例如: {{\"path\": \"...\", \"offset\": 301, \"limit\": 300}}\n\n\
                 === 前 {} 行 (共 {} 行) ===\n{}",
                meta.len() / 1024 / 1024,
                line_count,
                MAX_SIZE / 1024 / 1024,
                FILE_OUTLINE_PREVIEW_LINES,
                line_count,
                read_line_range(&path, 1, FILE_OUTLINE_PREVIEW_LINES)
            );
        }
        // Read with offset/limit
        if offset > 0 || limit > 0 {
            let start = if offset > 0 { offset } else { 1 };
            let end = if limit > 0 { start + limit - 1 } else { start + FILE_DEFAULT_READ_LIMIT };
            let total = count_lines(&path);
            let prefix = if meta.len() > OUTLINE_SIZE {
                format!("[lines {}-{} of {} — PARTIAL CONTENT, use offset/limit for more]\n", start, std::cmp::min(end, total), total)
            } else {
                String::new()
            };
            return format!("{}{}", prefix, read_line_range(&path, start, end));
        }
        // Normal read — always include header so agents know the scope
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Error: {}", e))
            .unwrap_or_else(|e| e);
        let total = content.lines().count();
        format!("[{} lines, {} KB — FULL CONTENT]\n{}", total, meta.len() / 1024, content)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn is_binary_file(path: &str) -> bool {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut buf = [0u8; BINARY_DETECT_BUFFER_SIZE];
        if let Ok(n) = f.read(&mut buf) {
            buf[..n].contains(&0)
        } else {
            false
        }
    } else {
        false
    }
}

pub(crate) fn count_lines(path: &str) -> usize {
    use std::io::{BufRead, BufReader};
    if let Ok(f) = std::fs::File::open(path) {
        BufReader::new(f).lines().count()
    } else {
        0
    }
}

pub(crate) fn read_line_range(path: &str, start: usize, end: usize) -> String {
    use std::io::{BufRead, BufReader};
    if let Ok(f) = std::fs::File::open(path) {
        BufReader::new(f).lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let line_num = i + 1;
                if line_num >= start && line_num <= end {
                    l.ok().map(|text| format!("{:>6}| {}", line_num, text))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "Error reading file".to_string()
    }
}

pub(crate) async fn tool_read_files(params: &serde_json::Value) -> String {
    const MAX_LINES: usize = MULTIFILE_MAX_LINES;
    const MAX_BYTES: usize = MULTIFILE_MAX_BYTES;
    if let Some(paths) = params["paths"].as_array() {
        let global_offset = params["offset"].as_u64().unwrap_or(0) as usize;
        let global_limit = params["limit"].as_u64().unwrap_or(0) as usize;
        let total = paths.len();
        let results: Vec<String> = paths.iter()
            .map(|p| {
                // Support both string paths and {path, offset, limit} objects
                let (path, offset, limit) = if p.is_object() {
                    let path = p["path"].as_str().unwrap_or("").to_string();
                    let off = p["offset"].as_u64().map(|v| v as usize).unwrap_or(global_offset);
                    let lim = p["limit"].as_u64().map(|v| v as usize).unwrap_or(global_limit);
                    (path, off, lim)
                } else {
                    (p.as_str().unwrap_or("").to_string(), global_offset, global_limit)
                };
                let path = normalize_file_path(&path);
                if path.is_empty() {
                    return "[FAIL] empty path".to_string();
                }
                // If offset/limit specified, use line-range reading
                if offset > 0 || limit > 0 {
                    let total_lines = count_lines(&path);
                    if total_lines == 0 {
                        return format!("[FAIL] {} — empty or unreadable", path);
                    }
                    let start = if offset > 0 { offset } else { 1 };
                    let end = if limit > 0 { start + limit - 1 } else { (start + MAX_LINES - 1).min(total_lines) };
                    let content = read_line_range(&path, start, end);
                    format!(
                        "[OK] {} (lines {}-{} of {} — PARTIAL)\n---\n{}",
                        path, start, end.min(total_lines), total_lines, content
                    )
                } else {
                    match std::fs::read_to_string(&path) {
                        Ok(c) => {
                            let lines = c.lines().count();
                            let bytes = c.len();
                            if lines > MAX_LINES || bytes > MAX_BYTES {
                                let display: String = c.lines().take(MAX_LINES).collect::<Vec<_>>().join("\n");
                                format!(
                                    "[OK] {} — TRUNCATED ({} lines/{} KB total, showing first {} lines. Use offset/limit for more)\n---\n{}",
                                    path, lines, bytes / 1024, MAX_LINES, display
                                )
                            } else {
                                format!("[OK] {} ({} lines, {} KB)\n---\n{}", path, lines, bytes / 1024, c)
                            }
                        }
                        Err(e) => format!("[FAIL] {} — {}", path, e),
                    }
                }
            })
            .collect();
        let label = if total == 1 { "1 file".to_string() } else { format!("{} files", total) };
        format!("=== {} files ({}) ===\n\n{}", total, label, results.join("\n\n"))
    } else {
        "Error: Invalid paths parameter".to_string()
    }
}

pub(crate) async fn tool_search_in_dir(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let pattern = params["pattern"].as_str().unwrap_or("").to_string();
    let file_type = params["file_type"].as_str().unwrap_or("").to_string();
    let context = params["context"].as_u64().unwrap_or(0).min(5) as usize;
    let use_regex = params.get("regex").and_then(|b| b.as_bool()).unwrap_or(false);
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let max_results = SEARCH_MAX_RESULTS;
        let show_limit = SEARCH_SHOW_LIMIT;
        let mut matches: Vec<(String, i32, String)> = Vec::new();
        if use_regex {
            let re = match regex::Regex::new(&pattern) {
                Ok(r) => r,
                Err(e) => return format!("Error: invalid regex pattern — {}", e),
            };
            search_recursive_regex(&path, &re, &file_type, 0, SEARCH_RECURSE_DEPTH, &mut matches, max_results);
        } else {
            let lower = pattern.to_lowercase();
            search_recursive_filtered(&path, &lower, &file_type, 0, SEARCH_RECURSE_DEPTH, &mut matches, max_results);
        }
        let total = matches.len();
        if total == 0 {
            return "No matches found".to_string();
        }
        // Build output with optional context lines
        let mut output_lines: Vec<String> = Vec::new();
        let shown = std::cmp::min(total, show_limit);
        for (file, line_num, line) in matches.iter().take(shown) {
            if context > 0 {
                let ctx = read_context_lines(file, *line_num, context);
                output_lines.push(format!("{}:{}: {}", file, line_num, line));
                for (cn, cl) in ctx {
                    output_lines.push(format!("{}-{}- {}", file, cn, cl));
                }
                output_lines.push("--".to_string());
            } else {
                output_lines.push(format!("{}:{}: {}", file, line_num, line));
            }
        }
        let suffix = if total > show_limit {
            format!("\n\n... showing {} of {} matches (use file_type to filter, e.g. \"rs\", \"vue\", \"ts\")", shown, total)
        } else { String::new() };
        format!("{}{}", output_lines.join("\n"), suffix)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

/// Enhanced search: supports regex, file type filtering, higher limits
fn search_recursive_regex(
    path: &str, re: &regex::Regex, file_type: &str,
    depth: usize, max_depth: usize, results: &mut Vec<(String, i32, String)>, limit: usize,
) {
    if depth > max_depth || results.len() >= limit { return; }
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if results.len() >= limit { break; }
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.starts_with('.') || matches!(file_name.as_str(), "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__") {
            continue;
        }
        if file_path.is_dir() {
            search_recursive_regex(&file_path.to_string_lossy(), re, file_type, depth + 1, max_depth, results, limit);
        } else if file_path.is_file() {
            // Filter by file extension
            if !file_type.is_empty() {
                let ext = file_path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                if ext.to_lowercase() != file_type.to_lowercase() { continue; }
            }
            if entry.metadata().map(|m| m.len() > 2_097_152).unwrap_or(false) { continue; }
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        results.push((file_path.to_string_lossy().to_string(), (i + 1) as i32, line.trim().to_string()));
                        if results.len() >= limit { return; }
                    }
                }
            }
        }
    }
}

/// Enhanced literal search: supports file type filtering, higher limits
fn search_recursive_filtered(
    path: &str, pattern: &str, file_type: &str,
    depth: usize, max_depth: usize, results: &mut Vec<(String, i32, String)>, limit: usize,
) {
    if depth > max_depth || results.len() >= limit { return; }
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if results.len() >= limit { break; }
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.starts_with('.') || matches!(file_name.as_str(), "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__") {
            continue;
        }
        if file_path.is_dir() {
            search_recursive_filtered(&file_path.to_string_lossy(), pattern, file_type, depth + 1, max_depth, results, limit);
        } else if file_path.is_file() {
            if !file_type.is_empty() {
                let ext = file_path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                if ext.to_lowercase() != file_type.to_lowercase() { continue; }
            }
            if entry.metadata().map(|m| m.len() > 2_097_152).unwrap_or(false) { continue; }
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                for (i, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(pattern) {
                        results.push((file_path.to_string_lossy().to_string(), (i + 1) as i32, line.trim().to_string()));
                        if results.len() >= limit { return; }
                    }
                }
            }
        }
    }
}

/// Read N lines of context around a match line
fn read_context_lines(file: &str, line_num: i32, context: usize) -> Vec<(i32, String)> {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line_num - 1).max(0) as usize;
    let start = idx.saturating_sub(context);
    let end = (idx + context + 1).min(lines.len());
    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate().take(end).skip(start) {
        if i != idx {
            result.push(((i + 1) as i32, line.trim().to_string()));
        }
    }
    result
}

pub(crate) async fn tool_get_env_info(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || get_env_info_sync(&path))
        .await
        .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_analyze_project_structure(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || analyze_project_sync(&path))
        .await
        .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_run_command(params: &serde_json::Value, session_id: i64, running_pids: Option<Arc<StdMutex<HashMap<i64, Vec<u32>>>>>) -> String {
    let command = params["command"].as_str().unwrap_or("").to_string();
    let cwd = params["path"].as_str().map(normalize_file_path);
    let timeout_secs = params["timeout"].as_u64().unwrap_or(COMMAND_TIMEOUT_SECS);
    if command.is_empty() {
        return "Error: command is required".to_string();
    }
    tokio::task::spawn_blocking(move || {
        // --- Shell routing strategy ---
        //
        // 1. Parse the command. If it contains unquoted shell metacharacters
        //    (|, >, <, &&) AND the program is NOT already a shell, wrap in
        //    the system shell. Direct execution would pass metacharacters as
        //    arguments, silently producing wrong output.
        // 2. If the program IS a known shell (cmd, powershell, sh, ...),
        //    execute directly — the LLM explicitly chose how to run it.
        //    Pipes/redirects inside its quoted arguments are the shell's job.
        // 3. Simple command → try direct first, fall back to system shell on
        //    spawn error (handles shell builtins like `echo`, `dir`).

        let (program, args) = parse_command(&command);
        if program.is_empty() {
            return "Error: could not parse command".to_string();
        }

        let needs_shell = has_shell_metacharacters(&command);

        if needs_shell && !is_shell_program(&program) {
            // Path 1: has pipes/redirects but not already wrapped in shell
            return execute_via_shell(
                &command, cwd.as_deref(), timeout_secs, session_id, running_pids.as_ref(),
            );
        }

        if is_shell_program(&program) {
            // Path 2: LLM explicitly chose a shell — execute directly
            let mut cmd = hidden_cmd(&program);
            cmd.args(&args);
            if let Some(ref dir) = cwd {
                cmd.current_dir(dir);
            }
            return output_with_timeout(
                &mut cmd, timeout_secs, session_id, running_pids.as_ref(),
            );
        }

        // Path 3: simple command — try direct, fall back to shell on error
        let mut cmd = hidden_cmd(&program);
        cmd.args(&args);
        if let Some(ref dir) = cwd {
            cmd.current_dir(dir);
        }
        let result = output_with_timeout(
            &mut cmd, timeout_secs, session_id, running_pids.as_ref(),
        );
        if !result.starts_with("Error spawning command:")
            && !result.starts_with("Error waiting")
        {
            return result;
        }

        // Fallback to system shell
        execute_via_shell(&command, cwd.as_deref(), timeout_secs, session_id, running_pids.as_ref())
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_write_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let content = params["content"].as_str().unwrap_or("").to_string();
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return format!("Error creating directory: {}", e);
            }
        }
        std::fs::write(&path, &content)
            .map_err(|e| format!("Error: {}", e))
            .map(|_| format!("Written: {} ({} lines, {} bytes)", path, content.lines().count(), content.len()))
            .unwrap_or_else(|e| e)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_write_files(params: &serde_json::Value) -> String {
    if let Some(files) = params["files"].as_array() {
        let total = files.len();
        let mut ok = 0usize;
        let mut fail = 0usize;
        let results: Vec<String> = files.iter()
            .filter_map(|f| f.as_object())
            .map(|obj| {
                let path = normalize_file_path(obj.get("path").and_then(|p| p.as_str()).unwrap_or(""));
                let content = obj.get("content").and_then(|c| c.as_str()).unwrap_or("");
                if path.is_empty() {
                    fail += 1;
                    return "[FAIL] empty path".to_string();
                }
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&path, content) {
                    Ok(_) => {
                        ok += 1;
                        format!("[OK] {} ({} lines, {} bytes)", path, content.lines().count(), content.len())
                    }
                    Err(e) => {
                        fail += 1;
                        format!("[FAIL] {} — {}", path, e)
                    }
                }
            })
            .collect();
        format!("=== {} files: {} ok, {} failed ===\n{}", total, ok, fail, results.join("\n"))
    } else {
        "Error: Invalid files parameter".to_string()
    }
}

pub(crate) async fn tool_find_replace_in_files(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let find = params["find"].as_str().unwrap_or("");
    let replace = params["replace"].as_str().unwrap_or("");
    let use_regex = params.get("use_regex").and_then(|b| b.as_bool()).unwrap_or(false);
    let path = path.to_string();
    let find = find.to_string();
    let replace = replace.to_string();
    tokio::task::spawn_blocking(move || {
        let mut count = 0;
        find_replace_recursive(&path, &find, &replace, use_regex, &mut count, 0, SEARCH_RECURSE_DEPTH);
        format!("Modified {} files", count)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_modify_files(params: &serde_json::Value) -> String {
    if let Some(files) = params["files"].as_array() {
        let total = files.len();
        let mut ok = 0usize;
        let mut fail = 0usize;
        let results: Vec<String> = files.iter()
            .filter_map(|f| f.as_object())
            .map(|obj| {
                let path = normalize_file_path(obj.get("path").and_then(|p| p.as_str()).unwrap_or(""));
                if path.is_empty() {
                    fail += 1;
                    return "[FAIL] empty path".to_string();
                }
                let original = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => { fail += 1; return format!("[FAIL] {} — {}", path, e); }
                };
                let mut content = original;
                if let Some(replacements) = obj.get("replacements").and_then(|r| r.as_array()) {
                    for rep in replacements.iter().filter_map(|r| r.as_object()) {
                        let find = rep.get("find").and_then(|f| f.as_str()).unwrap_or("");
                        let replace_with = rep.get("replace").and_then(|r| r.as_str()).unwrap_or("");
                        content = content.replace(find, replace_with);
                    }
                }
                if let Some(new_content) = obj.get("new_content").and_then(|c| c.as_str()) {
                    content = new_content.to_string();
                }
                match std::fs::write(&path, &content) {
                    Ok(_) => {
                        ok += 1;
                        format!("[OK] {} ({} lines, {} bytes)", path, content.lines().count(), content.len())
                    }
                    Err(e) => {
                        fail += 1;
                        format!("[FAIL] {} — {}", path, e)
                    }
                }
            })
            .collect();
        format!("=== {} files: {} ok, {} failed ===\n{}", total, ok, fail, results.join("\n"))
    } else {
        "Error: Invalid files parameter".to_string()
    }
}

pub(crate) async fn tool_get_file_info(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let is_dir = meta.is_dir();
                let size = meta.len();
                let ext = std::path::Path::new(&path)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "-".to_string());
                let modified = meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|dur| {
                        let secs = dur.as_secs();
                        let days = secs / 86400;
                        let (y, m, d) = days_to_ymd(days);
                        let time_of_day = secs % 86400;
                        let h = time_of_day / 3600;
                        let min = (time_of_day % 3600) / 60;
                        let s = time_of_day % 60;
                        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, s)
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                if is_dir {
                    let entry_count = std::fs::read_dir(&path)
                        .map(|e| e.filter_map(|e| e.ok()).count())
                        .unwrap_or(0);
                    format!(
                        "Type: directory\nEntries: {}\nSize: {} ({})\nModified: {}",
                        entry_count, size, format_size(size), modified
                    )
                } else {
                    let line_count = if size < 5_000_000 {
                        count_lines(&path)
                    } else { 0 };
                    format!(
                        "Type: file\nExtension: .{}\nLines: {}\nSize: {} ({})\nModified: {}",
                        ext, if line_count > 0 { line_count.to_string() } else { "> 5MB, skipped".to_string() },
                        size, format_size(size), modified
                    )
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_directory_tree(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let max_depth = params["max_depth"].as_i64().unwrap_or(2) as usize;
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut output = Vec::new();
        let root = std::path::Path::new(&path);
        if let Some(name) = root.file_name() {
            output.push(name.to_string_lossy().to_string());
        }
        tree_recursive(root, "", max_depth, &mut output);
        let n = output.len();
        format!("{} entries\n{}", n, output.join("\n"))
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn tree_recursive(dir: &std::path::Path, prefix: &str, max_depth: usize, output: &mut Vec<String>) {
    if max_depth == 0 { return; }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if a_dir && !b_dir { std::cmp::Ordering::Less }
        else if !a_dir && b_dir { std::cmp::Ordering::Greater }
        else { a.file_name().cmp(&b.file_name()) }
    });
    let skip = |name: &str| name.starts_with('.') || matches!(name, "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__");
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name().to_string_lossy().to_string();
        if skip(&name) { continue; }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let is_last = i == entries.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };
        output.push(format!("{}{}{}{}", prefix, branch, name, if is_dir { "/" } else { "" }));
        if is_dir {
            let path = entry.path();
            let new_prefix = format!("{}{}", prefix, child_prefix);
            tree_recursive(&path, &new_prefix, max_depth - 1, output);
        }
    }
}

pub(crate) async fn tool_glob(params: &serde_json::Value) -> String {
    let pattern = params["pattern"].as_str().unwrap_or("*").to_string();
    let base_path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let limit = params["limit"].as_i64().unwrap_or(GLOB_DEFAULT_LIMIT as i64) as usize;
    let file_type = params["file_type"].as_str().unwrap_or("").to_string();
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        let skip_dirs = |name: &str| name.starts_with('.') || matches!(name, "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__");
        let glob_re = if pattern.contains('*') || pattern.contains('?') {
            regex::Regex::new(&glob_to_regex(&pattern)).ok()
        } else {
            None
        };
        glob_recursive(&base_path, &glob_re, &pattern, &skip_dirs, &file_type, &mut results, limit);
        let total = results.len();
        let hit_limit = total >= limit;
        results.sort();
        let listing = results.iter().map(|p| p.strip_prefix(&base_path).unwrap_or(p).to_string()).collect::<Vec<_>>().join("\n");
        if hit_limit {
            format!("{}\n\n... {} results (limit reached — use narrower pattern, file_type, or increase limit)", listing, total)
        } else if listing.is_empty() {
            "No files found".to_string()
        } else {
            listing
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn glob_recursive(dir: &str, glob_re: &Option<regex::Regex>, raw_pattern: &str, skip_dirs: &dyn Fn(&str) -> bool, file_type: &str, results: &mut Vec<String>, limit: usize) {
    if results.len() >= limit { return; }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };
    for entry in entries {
        if results.len() >= limit { break; }
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = entry.file_type().ok();
        let is_dir = ft.as_ref().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if skip_dirs(&name) { continue; }
            let path_str = entry.path().to_string_lossy().to_string();
            glob_recursive(&path_str, glob_re, raw_pattern, skip_dirs, file_type, results, limit);
        } else {
            // Filter by file extension
            if !file_type.is_empty() {
                let ext = entry.path().extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                if ext.to_lowercase() != file_type.to_lowercase() { continue; }
            }
            let matches = match glob_re {
                Some(re) => re.is_match(&name),
                None => name.to_lowercase().contains(&raw_pattern.to_lowercase()),
            };
            if matches {
                results.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
}

pub(crate) fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(ch);
            }
            _ => re.push(ch),
        }
    }
    re.push('$');
    re
}

pub(crate) async fn tool_search_files(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let pattern = params["pattern"].as_str().unwrap_or("");
    let path = path.to_string();
    let pattern = pattern.to_lowercase();
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        search_files_recursive(&path, &pattern, &mut results, FILE_SEARCH_RESULT_LIMIT);
        let total = results.len();
        if total == 0 { "No files found".to_string() }
        else if total >= FILE_SEARCH_RESULT_LIMIT { format!("{}\n\n... {} results (limit reached — use narrower pattern)", results.join("\n"), total) }
        else { results.join("\n") }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn search_files_recursive(dir: &str, pattern: &str, results: &mut Vec<String>, limit: usize) {
    if results.len() >= limit { return; }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };
    for entry in entries {
        if results.len() >= limit { break; }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | ".git" | "dist" | "build") { continue; }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let path_str = entry.path().to_string_lossy().to_string();
        if is_dir {
            search_files_recursive(&path_str, pattern, results, limit);
        } else if name.to_lowercase().contains(pattern) {
            results.push(path_str);
        }
    }
}

pub(crate) async fn tool_edit_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let search = params["search"].as_str().unwrap_or("").to_string();
    let replace = params["replace"].as_str().unwrap_or("").to_string();

    if search.is_empty() {
        return "Error: search cannot be empty".to_string();
    }

    tokio::task::spawn_blocking(move || {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };

        // Auto-detect line endings and normalize search/replace to match
        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let search = search.replace("\r\n", "\n").replace('\n', line_ending);
        let replace = replace.replace("\r\n", "\n").replace('\n', line_ending);

        let count = content.matches(&search).count();
        if count == 0 {
            return format!(
                "Error: 未找到要替换的文本。文件可能已被修改，请先 read_file 确认当前内容。\n文件: {}\n查找: {}",
                path, truncate_str(&search, 200)
            );
        }
        if count > 1 {
            return format!(
                "Error: 找到 {} 处匹配，不够唯一。请增加更多上下文（前后各 3-5 行）使匹配唯一。", count
            );
        }

        let new_content = content.replacen(&search, &replace, 1);

        match std::fs::write(&path, &new_content) {
            Ok(_) => {
                let diff = compute_diff(&content, &new_content);
                format!("edited {} ({})\n{}", path, count_chars(&content, &new_content), diff)
            }
            Err(e) => format!("Error writing {}: {}", path, e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn count_chars(old: &str, new: &str) -> String {
    let old_len = old.len();
    let new_len = new.len();
    if new_len >= old_len {
        format!("{}->{} chars, +{}", old_len, new_len, new_len - old_len)
    } else {
        format!("{}->{} chars, -{}", old_len, new_len, old_len - new_len)
    }
}

pub(crate) fn compute_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = String::new();
    // Find changed region: first diff line to last diff line
    let mut first_diff = 0;
    while first_diff < old_lines.len() && first_diff < new_lines.len()
        && old_lines[first_diff] == new_lines[first_diff] {
        first_diff += 1;
    }
    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > first_diff && new_end > first_diff
        && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    // Show context (1 line before)
    let ctx_start = if first_diff > 0 { first_diff - 1 } else { 0 };
    result.push_str(&format!("@@ -{},{} +{},{} @@\n",
        ctx_start + 1, old_end - ctx_start, ctx_start + 1, new_end - ctx_start));
    for i in ctx_start..old_end {
        if i < old_lines.len() {
            result.push_str(&format!("-{}\n", old_lines[i]));
        }
    }
    for i in ctx_start..new_end {
        if i < new_lines.len() {
            result.push_str(&format!("+{}\n", new_lines[i]));
        }
    }
    result
}

pub(crate) fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

pub(crate) async fn tool_edit_lines(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let search = params["search"].as_str().map(|s| s.to_string());
    let mut start_line = params["start_line"].as_u64().unwrap_or(0) as usize;
    let end_line = params["end_line"].as_u64().map(|v| v as usize);
    let content = params["content"].as_str().map(|s| s.to_string());

    if path.is_empty() {
        return "Error: path is required".to_string();
    }

    // If search is provided, resolve it to a line number
    if let Some(ref needle) = search {
        if start_line == 0 {
            let file_content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return format!("Error reading {}: {}", path, e),
            };
            let found = file_content.lines().enumerate()
                .find(|(_, line)| line.contains(needle))
                .map(|(i, _)| i + 1);
            match found {
                Some(line_num) => {
                    start_line = line_num;
                }
                None => return format!(
                    "Error: search text not found in {}\nSearched: {}",
                    path, truncate_str(needle, 200)
                ),
            }
        }
    }

    if start_line == 0 {
        return "Error: start_line or search is required".to_string();
    }

    tokio::task::spawn_blocking(move || {
        let file_content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };
        let lines: Vec<&str> = file_content.lines().collect();
        let total = lines.len();

        if start_line > total + 1 {
            return format!("Error: start_line {} out of range (file has {} lines)", start_line, total);
        }

        let end = end_line.unwrap_or(start_line).min(total);
        let old_lines: Vec<String> = lines[start_line - 1..end].iter().map(|l| l.to_string()).collect();

        match (content.as_ref(), end_line) {
            (Some(text), Some(_)) => {
                // Replace: remove lines [start_line, end_line], insert content
                let new_lines: Vec<&str> = text.lines().collect();
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(new_lines.iter().map(|l| l.to_string()));
                result.extend(lines[end..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: replaced lines {}-{} with {} lines\n-{}\n+{}",
                        path, start_line, end, new_lines.len(),
                        old_lines.join("\n-"),
                        text.lines().collect::<Vec<_>>().join("\n+")),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (Some(text), None) => {
                // Insert: insert content before start_line
                let new_lines: Vec<&str> = text.lines().collect();
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(new_lines.iter().map(|l| l.to_string()));
                result.extend(lines[start_line - 1..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: inserted {} lines at line {}", path, new_lines.len(), start_line),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (None, Some(_)) => {
                // Delete: remove lines [start_line, end_line]
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(lines[end..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: deleted lines {}-{}\n-{}",
                        path, start_line, end,
                        old_lines.join("\n-")),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (None, None) => {
                "Error: either end_line or content must be provided".to_string()
            }
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_multi_edit(params: &serde_json::Value) -> String {
    let edits: Vec<(String, String, String)> = match params["edits"].as_array() {
        Some(arr) => arr.iter().map(|e| (
            e["path"].as_str().unwrap_or("").to_string(),
            e["search"].as_str().unwrap_or("").to_string(),
            e["replace"].as_str().unwrap_or("").to_string(),
        )).collect(),
        None => return r#"{"error": "edits array is required"}"#.to_string(),
    };

    tokio::task::spawn_blocking(move || {
        // Phase 1: Validate all edits
        let mut validated: Vec<(String, String, String, String)> = Vec::new();
        for (i, (path, search, replace)) in edits.iter().enumerate() {
            let path = path.clone();
            let search = search.clone();
            let replace = replace.clone();
            if path.is_empty() || search.is_empty() {
                return format!("Error: edit #{} has empty path or search", i + 1);
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return format!("Error reading {}: {}", path, e),
            };
            let count = content.matches(&search).count();
            if count == 0 {
                return format!(
                    "multi_edit: edit #{} failed — search text not found in {}. No files were modified.",
                    i + 1, path
                );
            }
            if count > 1 {
                return format!(
                    "multi_edit: edit #{} failed — search appears {} times in {}. Include more context. No files modified.",
                    i + 1, count, path
                );
            }
            validated.push((path, search, replace, content));
        }

        // Phase 2: Apply all edits (reuse content from Phase 1)
        let mut results = Vec::new();
        let mut original_contents: Vec<(String, String)> = Vec::new();
        for (path, search, replace, content) in &validated {
            original_contents.push((path.clone(), content.clone()));
            let new_content = content.replacen(search, replace, 1);
            match std::fs::write(path, &new_content) {
                Ok(_) => {
                    let diff = compute_diff(content, &new_content);
                    results.push(format!("{}: applied\n{}", path, diff));
                },
                Err(e) => {
                    // Rollback
                    for (rb_path, rb_content) in &original_contents {
                        let _ = std::fs::write(rb_path, rb_content);
                    }
                    return format!("multi_edit: write failed for {}: {}. All files rolled back.", path, e);
                }
            }
        }
        format!("multi_edit: {} edits applied across {} files\n\n{}",
            validated.len(),
            validated.iter().map(|(p,_,_,_)| p).collect::<std::collections::HashSet<_>>().len(),
            results.join("\n"))
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_create_directory(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        let existed = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
        match std::fs::create_dir_all(&path) {
            Ok(_) => {
                if existed { format!("Already exists: {}", path) }
                else { format!("Created: {}", path) }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_move_file(params: &serde_json::Value) -> String {
    let source = normalize_file_path(params["source"].as_str().unwrap_or(""));
    let destination = normalize_file_path(params["destination"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        match std::fs::rename(&source, &destination) {
            Ok(_) => format!("Moved: {} -> {}", source, destination),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_delete_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.is_dir() {
                return "Error: is a directory. Use a different tool to delete directories.".to_string();
            }
        }
        match std::fs::remove_file(&path) {
            Ok(_) => format!("[OK] Deleted: {}", path),
            Err(e) => format!("[FAIL] {}: {}", path, e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_copy_file_fn(params: &serde_json::Value) -> String {
    let source = normalize_file_path(params["source"].as_str().unwrap_or(""));
    let destination = normalize_file_path(params["destination"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        copy_recursive(&source, &destination)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn copy_recursive(src: &str, dst: &str) -> String {
    let src_path = std::path::Path::new(src);
    if !src_path.exists() {
        return format!("Error: source '{}' does not exist", src);
    }
    if src_path.is_dir() {
        match std::fs::create_dir_all(dst) {
            Ok(_) => {},
            Err(e) => return format!("Error: {}", e),
        }
        let entries = match std::fs::read_dir(src) {
            Ok(e) => e,
            Err(e) => return format!("Error: {}", e),
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let sub_src = src_path.join(&name);
            let sub_dst = std::path::Path::new(dst).join(&name);
            let result = copy_recursive(&sub_src.to_string_lossy(), &sub_dst.to_string_lossy());
            if result.starts_with("Error") { return result; }
        }
        format!("Copied directory: {} -> {}", src, dst)
    } else {
        match std::fs::copy(src, dst) {
            Ok(_) => format!("Copied: {} -> {}", src, dst),
            Err(e) => format!("Error: {}", e),
        }
    }
}

pub(crate) async fn tool_web_fetch(params: serde_json::Value) -> String {
    let url = params["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return "Error: url is required".to_string();
    }
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(crate::SEARCH_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(e) => return format!("Error building client: {}", e),
        };
        match client.get(&url).send() {
            Ok(resp) => match resp.text() {
                Ok(html) => {
                    let text = html_to_text(&html);
                    if text.len() > WEB_FETCH_MAX_CHARS {
                        format!("{}...\n[truncated at 32K chars]", &text[..WEB_FETCH_MAX_CHARS])
                    } else {
                        text
                    }
                }
                Err(e) => format!("Error reading response: {}", e),
            },
            Err(e) => format!("Error fetching URL: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) fn html_to_text(html: &str) -> String {
    // Single-pass byte-level strip: no clone, no O(n²) string mutation
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut in_tag = false;
    let mut i = 0;
    while i < len {
        // Check for <script or <style block start
        if !in_tag && i + 7 < len && &bytes[i..i+7] == b"<script" {
            // Skip until </script>
            i += 7;
            while i + 8 < len {
                if &bytes[i..i+9] == b"</script>" { i += 9; break; }
                i += 1;
            }
            continue;
        }
        if !in_tag && i + 6 < len && &bytes[i..i+6] == b"<style" {
            i += 6;
            while i + 7 < len {
                if &bytes[i..i+8] == b"</style>" { i += 8; break; }
                i += 1;
            }
            continue;
        }
        match bytes[i] {
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            b if (!in_tag && !result.is_empty() || b != b' ' || result.ends_with(' '))
                // Don't lead with space
                && (b != b'\n' || !result.ends_with('\n')) => {
                    result.push(b as char);
                }
            _ => {}
        }
        i += 1;
    }
    // Trim and collapse blank lines
    let out: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    out.join("\n")
}

pub(crate) async fn tool_run_background(params: &serde_json::Value, session_id: i64) -> String {
    let command = params["command"].as_str().unwrap_or("");
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let wait_sec = params["wait_sec"].as_i64().unwrap_or(3) as u64;
    if command.is_empty() {
        return "Error: command is required".to_string();
    }
    eprintln!("[run_background] Spawning: {} (cwd: {})", command, path);

    // Temp dir for output capture
    let tmp_dir = user_home_dir().join(".minimaxcode").join("tmp");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let out_file = tmp_dir.join(format!("bg_out_{}.txt", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
    let err_file = tmp_dir.join(format!("bg_err_{}.txt", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() + 1));

    // --- Shell routing (same strategy as tool_run_command) ---
    let needs_shell = has_shell_metacharacters(command);

    // Build the command to spawn. For metacharacter commands, always wrap in shell.
    // For simple commands, try direct first with fallback to shell on spawn error.
    let make_cmd = |program: &str, args: &[String]| -> std::process::Command {
        let mut c = hidden_cmd(program);
        c.args(args);
        c.current_dir(&path);
        c.stdout(std::process::Stdio::piped());
        c.stderr(std::process::Stdio::piped());
        c
    };

    let (program, args): (String, Vec<String>) = if needs_shell {
        if cfg!(windows) {
            ("cmd".into(), vec!["/d".into(), "/s".into(), "/c".into(), command.to_string()])
        } else {
            ("sh".into(), vec!["-c".into(), command.to_string()])
        }
    } else {
        let (p, a) = parse_command(command);
        (p, a)
    };

    if program.is_empty() {
        return "Error: could not parse command".to_string();
    }

    // Spawn helper: returns (task_id, pid) on success
    let try_spawn = |prog: &str, args: &[String],
                      out_clone: &std::path::PathBuf,
                      err_clone: &std::path::PathBuf|
     -> Result<(u64, u32), String>
    {
        let mut cmd = make_cmd(prog, args);
        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn: {}", e))?;
        let pid = child.id();
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let task_id = crate::background_tasks::register_task(
            session_id, pid, command,
            &out_clone.to_string_lossy(),
            &err_clone.to_string_lossy(),
        );

        // Capture stdout → file in real-time
        if let Some(stdout) = child_stdout {
            let out_path = out_clone.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader, Write};
                let mut reader = BufReader::new(stdout);
                if let Ok(mut file) = std::fs::File::create(&out_path) {
                    let mut buf = Vec::new();
                    while reader.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
                        let line = String::from_utf8_lossy(&buf);
                        let _ = writeln!(file, "{}", line.trim_end());
                        let _ = file.flush();
                        buf.clear();
                    }
                    if !buf.is_empty() {
                        let _ = writeln!(file, "{}", String::from_utf8_lossy(&buf).trim_end());
                    }
                }
            });
        }

        // Capture stderr → file in real-time
        if let Some(stderr) = child_stderr {
            let err_path = err_clone.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader, Write};
                let mut reader = BufReader::new(stderr);
                if let Ok(mut file) = std::fs::File::create(&err_path) {
                    let mut buf = Vec::new();
                    while reader.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
                        let line = String::from_utf8_lossy(&buf);
                        let _ = writeln!(file, "{}", line.trim_end());
                        let _ = file.flush();
                        buf.clear();
                    }
                    if !buf.is_empty() {
                        let _ = writeln!(file, "{}", String::from_utf8_lossy(&buf).trim_end());
                    }
                }
            });
        }

        // Reaper: wait for exit, write exit code, mark task done
        let out_reap = out_clone.clone();
        std::thread::spawn(move || {
            let status = child.wait();
            let exit_code = status.as_ref().ok().and_then(|s| s.code());
            if let Ok(ref f) = std::fs::OpenOptions::new().append(true).open(&out_reap) {
                use std::io::Write;
                let _ = writeln!(&mut &*f, "\n--- process exit: {:?} ---", exit_code);
            }
            crate::background_tasks::task_done(task_id, exit_code);
        });

        Ok((task_id, pid))
    };

    let (task_id, pid) = match try_spawn(&program, &args, &out_file, &err_file) {
        Ok(id) => id,
        Err(_first_err) => {
            // If direct spawn failed and we didn't already use shell, retry with shell
            if !needs_shell && !is_shell_program(&program) {
                let shell_prog = if cfg!(windows) { "cmd" } else { "sh" };
                let shell_args: Vec<String> = if cfg!(windows) {
                    vec!["/d".into(), "/s".into(), "/c".into(), command.to_string()]
                } else {
                    vec!["-c".into(), command.to_string()]
                };
                match try_spawn(shell_prog, &shell_args, &out_file, &err_file) {
                    Ok(id) => id,
                    Err(e) => return json!({"error": e}).to_string(),
                }
            } else {
                return json!({"error": _first_err}).to_string();
            }
        }
    };

    // Small wait for initial output
    tokio::time::sleep(std::time::Duration::from_secs(wait_sec)).await;
    let startup = std::fs::read_to_string(&out_file).unwrap_or_default();

    json!({
        "success": true,
        "task_id": task_id,
        "pid": pid,
        "out_file": out_file.to_string_lossy().to_string(),
        "err_file": err_file.to_string_lossy().to_string(),
        "startup_output": startup,
        "message": format!("Background process started (PID {}), output: {}", pid, out_file.to_string_lossy())
    }).to_string()
}

pub(crate) async fn tool_job_output(params: &serde_json::Value) -> String {
    let pid = params["job_id"].as_i64().unwrap_or(0) as u32;
    let tail = params["tail_lines"].as_i64().unwrap_or(200) as usize;
    // Allow reading output from file directly (preferred over PID for live output)
    let out_file = params["out_file"].as_str().unwrap_or("");

    if !out_file.is_empty() && std::path::Path::new(out_file).exists() {
        let content = match std::fs::read_to_string(out_file) {
            Ok(c) => c,
            Err(e) => return format!("读取输出文件失败: {}", e),
        };
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let tail_start = total.saturating_sub(tail);
        let tail_text: String = lines[tail_start..].iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        // Check if process is still running
        let status = if cfg!(windows) {
            hidden_cmd("tasklist")
                .args(["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        } else {
            hidden_cmd("ps")
                .args(["-p", &pid.to_string()])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        };

        return format!("{} ({} / {} lines)\n{}",
            if status { "运行中" } else { "已结束" },
            tail_text.lines().count(),
            total,
            tail_text.trim());
    }

    if pid == 0 {
        return "Error: 需要 job_id 或 out_file 参数".to_string();
    }
    // Legacy: try to get process info via tasklist/ps
    if cfg!(windows) {
        let output = hidden_cmd("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output();
        match output {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.contains(&pid.to_string()) {
                    format!("Process {} is running.\n{}", pid, text)
                } else {
                    format!("Process {} is not running. 如果之前返回了 out_file，请用 out_file 参数读取输出。", pid)
                }
            }
            Err(_) => format!("Process {} status unknown.", pid),
        }
    } else {
        let output = hidden_cmd("ps")
            .args(["-p", &pid.to_string(), "-o", "pid,stat,command"])
            .output();
        match output {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.contains(&pid.to_string()) {
                    format!("Process {} is running:\n{}", pid, text)
                } else {
                    format!("Process {} is not running. 如果之前返回了 out_file，请用 out_file 参数读取输出。", pid)
                }
            }
            Err(_) => format!("Process {} status unknown.", pid),
        }
    }
}

pub(crate) async fn tool_list_jobs(_params: &serde_json::Value) -> String {
    if cfg!(windows) {
        match hidden_cmd("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()
        {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.trim().is_empty() {
                    "No running processes detected.".to_string()
                } else {
                    format!("Running processes:\n{}", text.lines().take(PROCESS_LIST_MAX_LINES).collect::<Vec<_>>().join("\n"))
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    } else {
        match hidden_cmd("ps").args(["-eo", "pid,comm,stat"]).output() {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.trim().is_empty() {
                    "No running processes detected.".to_string()
                } else {
                    format!("Running processes:\n{}", text.lines().take(PROCESS_LIST_MAX_LINES).collect::<Vec<_>>().join("\n"))
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

pub(crate) async fn tool_spawn_process(params: &serde_json::Value) -> String {
    let command = params["command"].as_str().unwrap_or("");
    let cwd = params.get("path").and_then(|p| p.as_str());
    if command.is_empty() {
        return "Error: command is required".to_string();
    }
    let command = command.to_string();
    let cwd = cwd.map(|s| s.to_string());
    tokio::task::spawn_blocking(move || {
        let (program, args) = parse_command(&command);
        if program.is_empty() {
            return "Error: could not parse command".to_string();
        }
        let mut cmd = hidden_cmd(&program);
        cmd.args(&args);
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        match cmd.spawn() {
            Ok(child) => format!("Process started with PID: {}", child.id()),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

pub(crate) async fn tool_kill_process(params: &serde_json::Value) -> String {
    let pid = params["pid"].as_i64().unwrap_or(0) as u32;
    tokio::task::spawn_blocking(move || {
        let output = if cfg!(windows) {
            hidden_cmd("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output()
        } else {
            hidden_cmd("kill")
                .args(["-9", &pid.to_string()])
                .output()
        };
        match output {
            Ok(o) => {
                if o.status.success() {
                    "Process killed".to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

/// Vision analysis using Anthropic Messages API for custom providers.
pub(crate) async fn vision_anthropic(prompt: &str, image_url: &str, api_key: &str, api_url: &str, model: &str) -> String {
    let (mime, base64_data) = match resolve_image_base64(image_url) {
        Ok((m, b)) => (m, b),
        Err(e) => return format!(r#"{{"success": false, "error": "Failed to load image: {}"}}"#, e),
    };

    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": VISION_MAX_TOKENS,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime,
                        "data": base64_data
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }]
    });

    let client = reqwest::Client::new();
    let messages_path = format!("{}/v1/messages", api_url.trim_end_matches('/'));
    match client.post(&messages_path)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(VISION_HTTP_TIMEOUT_SECS))
        .send()
        .await
    {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    let text = data["content"].as_array()
                        .and_then(|arr| arr.iter().find(|b| b["type"] == "text"))
                        .and_then(|b| b["text"].as_str())
                        .unwrap_or("");
                    serde_json::to_string(&serde_json::json!({
                        "success": true,
                        "content": text
                    })).unwrap_or_default()
                }
                Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
            }
        }
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

pub(crate) fn resolve_image_base64(image_url: &str) -> Result<(String, String), String> {
    let data = if image_url.starts_with("http://") || image_url.starts_with("https://") {
        reqwest::blocking::get(image_url)
            .map_err(|e| e.to_string())?
            .bytes()
            .map_err(|e| e.to_string())?
            .to_vec()
    } else {
        std::fs::read(image_url).map_err(|e| format!("Cannot read {}: {}", image_url, e))?
    };
    let mime = if data.len() >= 3 && &data[0..3] == b"\xFF\xD8\xFF" { "image/jpeg" }
        else if data.len() >= 4 && &data[0..4] == b"\x89PNG" { "image/png" }
        else if data.len() >= 4 && &data[0..4] == b"RIFF" { "image/webp" }
        else { "image/png" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok((mime.to_string(), b64))
}

pub(crate) async fn tool_web_search(params: serde_json::Value, api_key: String, api_url: String, provider: String) -> String {
    let query = params["query"].as_str().unwrap_or("").to_string();
    if query.is_empty() {
        return r#"{"success": false, "error": "query is required"}"#.to_string();
    }

    // Custom providers don't have web_search registered (agent uses MCP tool instead).
    // If we reach here with a custom provider, something went wrong — guide agent to MCP.
    if provider == "custom" {
        return r#"{"success": false, "error": "Web search is not available for this provider. Connect an MCP search tool to enable it."}"#.to_string();
    }

    // MiniMax provider: use MiniMax's built-in search API
    let search_url = format!("{}/v1/coding_plan/search", api_url);
    tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        let resp = client.post(&search_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({ "q": query }))
            .timeout(std::time::Duration::from_secs(SEARCH_TIMEOUT_SECS))
            .send();

        match resp {
            Ok(response) => {
                match response.json::<serde_json::Value>() {
                    Ok(data) => {
                        let results = data.get("organic")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter().take(10).map(|item| {
                                    serde_json::json!({
                                        "title": item.get("title").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "link": item.get("link").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "snippet": item.get("snippet").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "date": item.get("date").unwrap_or(&serde_json::Value::String("".to_string()))
                                    })
                                }).collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        serde_json::to_string(&serde_json::json!({
                            "success": true,
                            "results": results,
                            "related_searches": data.get("related_searches").cloned().unwrap_or(serde_json::Value::Array(vec![]))
                        })).unwrap_or_else(|_| r#"{"success": false, "error": "JSON serialization failed"}"#.to_string())
                    }
                    Err(e) => serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": format!("Failed to parse response: {}", e)
                    })).unwrap()
                }
            }
            Err(e) => serde_json::to_string(&serde_json::json!({
                "success": false,
                "error": format!("Request failed: {}", e)
            })).unwrap()
        }
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

pub(crate) async fn tool_understand_image(params: serde_json::Value, api_key: String, api_url: String, model: String, provider: String) -> String {
    let prompt = params["prompt"].as_str().unwrap_or("").to_string();
    let image_url = params["image_url"].as_str().unwrap_or("").to_string();

    if prompt.is_empty() || image_url.is_empty() {
        return r#"{"success": false, "error": "prompt and image_url are required"}"#.to_string();
    }

    if provider == "custom" {
        return vision_anthropic(&prompt, &image_url, &api_key, &api_url, &model).await;
    }

    // MiniMax provider: use MiniMax vision API
    tokio::task::spawn_blocking(move || {
        let image_data = resolve_image(&image_url);

        let client = reqwest::blocking::Client::new();
        let resp = client.post(format!("{}/v1/coding_plan/vlm", DEFAULT_API_URL))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "prompt": prompt,
                "image_url": image_data
            }))
            .timeout(std::time::Duration::from_secs(VLM_TIMEOUT_SECS))
            .send();

        match resp {
            Ok(response) => {
                match response.json::<serde_json::Value>() {
                    Ok(data) => {
                        let description = data.get("content")
                            .and_then(|c| c.as_str())
                            .unwrap_or("No description available");
                        serde_json::to_string(&serde_json::json!({
                            "success": true,
                            "description": description
                        })).unwrap_or_else(|_| r#"{"success": false, "error": "JSON serialization failed"}"#.to_string())
                    }
                    Err(e) => serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": format!("Failed to parse response: {}", e)
                    })).unwrap()
                }
            }
            Err(e) => serde_json::to_string(&serde_json::json!({
                "success": false,
                "error": format!("Request failed: {}", e)
            })).unwrap()
        }
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

pub(crate) async fn tool_mcp_reload(
    _params: &serde_json::Value,
    mcp_service: Arc<RwLock<McpService>>,
    skill_service: Arc<SkillService>,
    db: Arc<StdMutex<Connection>>,
) -> String {
    skill_service.load_all_skills().await;
    let workspace: Option<String> = {
        if let Ok(conn) = db.lock() {
            conn.query_row("SELECT workspace FROM app_config", [], |row| row.get(0)).ok()
        } else {
            None
        }
    };
    let mcp = mcp_service.read().await;
    let statuses = mcp.reload(workspace.as_deref()).await;
    let tool_count = mcp.get_all_tools().await.len();
    let config_info = if let Some(ref ws) = workspace {
        format!("全局 + 项目 ({})", ws)
    } else {
        "全局".to_string()
    };
    let connected = statuses.iter().filter(|s| s.status == "connected").count();
    let failed: Vec<_> = statuses.iter().filter(|s| s.status == "failed").collect();
    let disabled: Vec<_> = statuses.iter().filter(|s| s.status == "disabled").collect();
    let mut result = serde_json::json!({
        "success": true,
        "message": format!("MCP 配置已重载 ({}): {} 个服务器连接成功，{} 个工具可用", config_info, connected, tool_count)
    });
    if !failed.is_empty() {
        result["failed"] = serde_json::Value::Array(
            failed.iter().map(|s| serde_json::json!({"name": s.name, "error": s.error})).collect()
        );
    }
    if !disabled.is_empty() {
        result["disabled"] = serde_json::Value::Array(
            disabled.iter().map(|s| serde_json::json!({"name": s.name})).collect()
        );
    }
    serde_json::to_string(&result).unwrap_or_default()
}

pub(crate) async fn tool_skill(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let name = params["name"].as_str().unwrap_or("");
    if name.is_empty() {
        return r#"{"success": false, "error": "Skill name is required"}"#.to_string();
    }

    match skill_service.get_skill_content(name).await {
        Some(content) => {
            let skill = skill_service.get_skill(name).await;
            format!(r#"{{"success": true, "skill": {{"name": "{}", "content": {}, "allowed_tools": {:?}}}, "scripts": {:?}, "references": {:?}}}"#,
                name,
                serde_json::to_string(&content).unwrap_or_default(),
                skill.as_ref().map(|s| s.allowed_tools.clone()).unwrap_or_default(),
                skill.as_ref().map(|s| s.scripts.clone()).unwrap_or_default(),
                skill.as_ref().map(|s| s.references.clone()).unwrap_or_default())
        }
        None => format!(r#"{{"success": false, "error": "Skill '{}' not found"}}"#, name),
    }
}

pub(crate) async fn tool_list_builtin_skills(_tool_name: &str, _params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let skills = skill_service.list_skills(Some("builtin")).await;
    serde_json::to_string(&skills).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) async fn tool_list_user_skills(_tool_name: &str, _params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    // List external skills: user skills + project skills
    let user_skills = skill_service.list_skills(Some("user")).await;
    let project_skills = skill_service.list_skills(Some("project")).await;
    let mut all = Vec::new();
    all.extend(user_skills);
    all.extend(project_skills);
    serde_json::to_string(&all).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) async fn tool_match_skills(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let query = params["query"].as_str().unwrap_or("");
    let top_k = params.get("top_k").and_then(|k| k.as_u64()).unwrap_or(3) as usize;
    let mut matches = skill_service.match_skills(query, top_k * 2).await;
    // Prioritize external (user/project) skills: boost their score by 1.5x
    for m in &mut matches {
        if m.source != "builtin" {
            m.score *= 1.5;
        }
    }
    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    matches.truncate(top_k);
    serde_json::to_string(&matches).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) async fn tool_execute_skill(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let name = params["name"].as_str().unwrap_or("");
    let script = params.get("script").and_then(|s| s.as_str());
    if name.is_empty() {
        return r#"{"success": false, "error": "Skill name is required"}"#.to_string();
    }
    match skill_service.execute_skill(name, script).await {
        Ok(output) => format!(r#"{{"success": true, "output": {}}}"#, serde_json::to_string(&output).unwrap_or_default()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

pub(crate) async fn tool_read_knowledge(params: &serde_json::Value) -> String {
    let file_name = params["file_name"].as_str().unwrap_or("");

    if file_name.is_empty() {
        return r#"{"success": false, "error": "file_name is required"}"#.to_string();
    }

    let base = user_home_dir();
    // Read workspace from DB to derive project name
    let project_name = get_project_name();
    let path = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge").join(file_name);

    match std::fs::read_to_string(&path) {
        Ok(content) => format!(r#"{{"success": true, "content": {}}}"#, serde_json::to_string(&content).unwrap_or_default()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

pub(crate) async fn tool_write_knowledge(params: &serde_json::Value) -> String {
    let file_name = params["file_name"].as_str().unwrap_or("");
    let content = params["content"].as_str().unwrap_or("");

    if file_name.is_empty() {
        return r#"{"success": false, "error": "file_name is required"}"#.to_string();
    }

    let base = user_home_dir();
    let project_name = get_project_name();
    let dir = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge");
    let path = dir.join(file_name);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&path, content) {
        Ok(_) => format!(r#"{{"success": true, "path": "{}"}}"#, path.display()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

pub(crate) async fn tool_list_knowledge() -> String {
    tokio::task::spawn_blocking(|| {
        let base = user_home_dir();
        let project_name = get_project_name();
        let dir = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge");

        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = meta.len();
                        let modified = meta.modified()
                            .map(|t| format!("{:?}", t))
                            .unwrap_or_default();
                        files.push(serde_json::json!({
                            "name": name,
                            "size": size,
                            "modified": modified
                        }));
                    }
                }
            }
        }
        serde_json::to_string(&serde_json::json!({
            "success": true,
            "files": files
        })).unwrap_or_else(|_| r#"{"success": false, "error": "serialization error"}"#.to_string())
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

// ========== Helper Functions ==========

pub(crate) fn find_replace_recursive(path: &str, find: &str, replace: &str, use_regex: bool, count: &mut usize, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" {
            continue;
        }

        if file_path.is_dir() {
            find_replace_recursive(&file_path.to_string_lossy(), find, replace, use_regex, count, depth + 1, max_depth);
        } else if file_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let new_content = if use_regex {
                    match regex::Regex::new(find) {
                        Ok(re) => re.replace_all(&content, replace).to_string(),
                        Err(_) => content.replace(find, replace),
                    }
                } else {
                    content.replace(find, replace)
                };
                if new_content != content
                    && std::fs::write(&file_path, &new_content).is_ok() {
                        *count += 1;
                    }
            }
        }
    }
}

pub(crate) fn get_env_info_sync(repo_path: &str) -> String {
    let mut info = Vec::new();
    let proj_path = std::path::Path::new(repo_path);

    if proj_path.join("package.json").exists() {
        info.push("Project: Node.js/npm".to_string());
    }
    if proj_path.join("Cargo.toml").exists() {
        info.push("Project: Rust/Cargo".to_string());
    }
    if proj_path.join("requirements.txt").exists() || proj_path.join("pyproject.toml").exists() {
        info.push("Project: Python".to_string());
    }

    let output = hidden_cmd("git")
        .args(["-C", repo_path, "status", "--porcelain"])
        .output();
    if let Ok(o) = output {
        let lines = String::from_utf8_lossy(&o.stdout);
        if lines.trim().is_empty() {
            info.push("Git: Clean".to_string());
        } else {
            info.push("Git: Modified".to_string());
        }
    }

    info.join("\n")
}

pub(crate) fn analyze_project_sync(repo_path: &str) -> String {
    let path = std::path::Path::new(repo_path);
    let mut info = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                info.push(format!("[DIR] {}", name));
            } else {
                info.push(format!("[FILE] {}", name));
            }
        }
    }

    info.join("\n")
}

// ========== Tool Definitions ==========

pub(crate) fn resolve_image(image_url: &str) -> String {
    // Already a data URI — pass through
    if image_url.starts_with("data:") {
        return image_url.to_string();
    }

    // Local file path — read and encode
    let path = std::path::Path::new(image_url);
    if path.exists() {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.len() > IMAGE_MAX_SIZE_BYTES as usize {
                return String::new(); // Too large (>50MB)
            }
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png")
                .to_lowercase();
            let mime = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                _ => "image/png",
            };
            return format!("data:{};base64,{}", mime, base64_encode(&bytes));
        }
    }

    // HTTP URL — fetch and encode
    if image_url.starts_with("http://") || image_url.starts_with("https://") {
        if let Ok(resp) = reqwest::blocking::get(image_url) {
            if let Ok(bytes) = resp.bytes() {
                if bytes.len() > IMAGE_MAX_SIZE_BYTES as usize {
                    return String::new();
                }
                let mime = "image/jpeg"; // default
                return format!("data:{};base64,{}", mime, base64_encode(&bytes));
            }
        }
    }

    // Fallback: pass as-is
    image_url.to_string()
}

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let val = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((val >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((val >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((val >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(val & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub(crate) fn user_home_dir() -> std::path::PathBuf {
    if cfg!(windows) {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOMEDRIVE").and_then(|hd| std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp))))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::env::var("HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from("."))
    }
}

/// Normalize a file path for the current OS.
/// - On Windows: converts Unix-style `/X/path` to `X:/path` and normalizes separators
/// - Handles both backslash and forward-slash paths
/// - Resolves `~` to home directory
pub(crate) fn normalize_file_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');

    // Expand ~ to home directory
    let expanded = if trimmed.starts_with('~') {
        let home = user_home_dir();
        home.join(trimmed.trim_start_matches('~').trim_start_matches('/').trim_start_matches('\\'))
            .to_string_lossy()
            .to_string()
    } else {
        trimmed.to_string()
    };

    if cfg!(windows) {
        // Convert Unix-style /X/path to X:/path (e.g. /e/foo → E:/foo)
        let normalized = if expanded.len() >= 2
            && expanded.starts_with('/')
            && expanded.as_bytes().get(1).is_some_and(|b| b.is_ascii_alphabetic())
            && expanded.as_bytes().get(2).is_none_or(|b| *b == b'/' || *b == b'\\')
        {
            let drive = (expanded.as_bytes()[1] as char).to_ascii_uppercase();
            format!("{}:/{}", drive, &expanded[3..])
        } else {
            expanded
        };
        // Normalize separators to backslash for Windows
        normalized.replace('/', "\\")
    } else {
        // On Unix: normalize backslashes to forward slashes
        expanded.replace('\\', "/")
    }
}

pub(crate) fn get_project_name() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Try to find a project root marker nearby
    let markers = [
        "package.json", "Cargo.toml", "go.mod", "pyproject.toml", "setup.py",
        "Gemfile", "pom.xml", "build.gradle", "build.gradle.kts", "CMakeLists.txt",
        "Makefile",
        // .git last — avoid hijacking by a parent repo (e.g. dotfiles)
        ".git",
    ];
    let project_root = find_project_root(&cwd, &markers);

    project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub(crate) fn find_project_root(start: &std::path::Path, markers: &[&str]) -> std::path::PathBuf {
    let mut current = if start.is_dir() { start.to_path_buf() } else { start.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
    loop {
        for marker in markers {
            if current.join(marker).exists() {
                return current;
            }
        }
        if let Some(parent) = current.parent() {
            if parent == current { break; }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    // Fallback: use the start directory
    if start.is_dir() { start.to_path_buf() } else { start.parent().map(|p| p.to_path_buf()).unwrap_or_default() }
}

// ========== Worktree Tools ==========

pub(crate) async fn tool_create_worktree(
    params: &serde_json::Value,
    caller_session_id: i64,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let branch_name = params["branch_name"].as_str().unwrap_or("");
    let base_dir = params["base_dir"].as_str().unwrap_or(".");

    if branch_name.is_empty() {
        return json!({"error": "branch_name is required"}).to_string();
    }

    let branch = branch_name.to_string();
    let base = normalize_file_path(base_dir);
    let db_clone = db.clone();

    let result = tokio::task::spawn_blocking(move || {
        let worktree_dir = format!("../work-{}", branch.replace('/', "-"));

        let output = hidden_cmd("git")
            .args(["worktree", "add", &worktree_dir, "-b", &branch])
            .current_dir(&base)
            .output()
            .map_err(|e| format!("Failed to run git worktree: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree add failed: {}", stderr));
        }

        let abs_path = std::path::Path::new(&base)
            .join(&worktree_dir)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| format!("{}/{}", base.trim_end_matches('/'), worktree_dir.trim_start_matches("../")));

        // Store in DB
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {}", e))?;
        conn.execute(
            "UPDATE agent_session SET worktree_path = ?1 WHERE id = ?2",
            rusqlite::params![abs_path, caller_session_id],
        ).map_err(|e| format!("Failed to update session: {}", e))?;

        Ok(abs_path)
    }).await;

    match result {
        Ok(Ok(path)) => json!({"success": true, "worktree_path": path}).to_string(),
        Ok(Err(e)) => json!({"error": e}).to_string(),
        Err(e) => json!({"error": format!("Task error: {}", e)}).to_string(),
    }
}

pub(crate) async fn tool_merge_worktree(
    params: &serde_json::Value,
    caller_session_id: i64,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let worktree_path = params["worktree_path"].as_str().unwrap_or("");
    let message = params["message"].as_str().unwrap_or("Merge worktree");

    if worktree_path.is_empty() {
        return json!({"error": "worktree_path is required"}).to_string();
    }

    let wt_path = worktree_path.to_string();
    let msg = message.to_string();
    let db_clone = db.clone();

    let result = tokio::task::spawn_blocking(move || {
        // git add -A
        let add_out = hidden_cmd("git")
            .args(["add", "-A"])
            .current_dir(&wt_path)
            .output()
            .map_err(|e| format!("git add failed: {}", e))?;
        if !add_out.status.success() {
            return Err(format!("git add failed: {}", String::from_utf8_lossy(&add_out.stderr)));
        }

        // git commit
        let commit_out = hidden_cmd("git")
            .args(["commit", "-m", &msg])
            .current_dir(&wt_path)
            .output()
            .map_err(|e| format!("git commit failed: {}", e))?;
        if !commit_out.status.success() {
            let stderr = String::from_utf8_lossy(&commit_out.stderr);
            if !stderr.contains("nothing to commit") {
                return Err(format!("git commit failed: {}", stderr));
            }
        }

        // Get branch name
        let branch_out = hidden_cmd("git")
            .args(["branch", "--show-current"])
            .current_dir(&wt_path)
            .output()
            .map_err(|e| format!("git branch failed: {}", e))?;
        let branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();

        // Get base dir (parent of worktree)
        let base_dir = std::path::Path::new(&wt_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        // checkout main and merge
        let _ = hidden_cmd("git")
            .args(["checkout", "main"])
            .current_dir(&base_dir)
            .output();

        let merge_out = hidden_cmd("git")
            .args(["merge", &branch])
            .current_dir(&base_dir)
            .output()
            .map_err(|e| format!("git merge failed: {}", e))?;
        if !merge_out.status.success() {
            return Err(format!("git merge failed: {}", String::from_utf8_lossy(&merge_out.stderr)));
        }

        // Clear worktree_path
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {}", e))?;
        conn.execute(
            "UPDATE agent_session SET worktree_path = NULL WHERE id = ?1",
            [caller_session_id],
        ).map_err(|e| format!("Failed to update session: {}", e))?;

        Ok(format!("Merged branch '{}' into main", branch))
    }).await;

    match result {
        Ok(Ok(msg)) => json!({"success": true, "message": msg}).to_string(),
        Ok(Err(e)) => json!({"error": e}).to_string(),
        Err(e) => json!({"error": format!("Task error: {}", e)}).to_string(),
    }
}

/// Normalize a file path with optional worktree prefix.
/// If worktree_path is Some, prepends it to the relative path.
pub(crate) fn normalize_file_path_with_worktree(path: &str, worktree_path: Option<&str>) -> String {
    match worktree_path {
        Some(wp) if !wp.is_empty() => {
            let normalized = normalize_file_path(path);
            // Don't prefix if path is already absolute
            if std::path::Path::new(&normalized).is_absolute() {
                normalized
            } else {
                format!("{}/{}", wp.trim_end_matches('/'), normalized.trim_start_matches('/').trim_start_matches('\\'))
            }
        }
        _ => normalize_file_path(path),
    }
}

pub(crate) fn tool_reason(tool: &str, file: Option<&str>, cmd: Option<&str>) -> String {
    match tool {
        "run_command" | "run_background" => {
            format!("Run: {}", cmd.unwrap_or("unknown command"))
        }
        "write_file" | "write_files" | "edit_file" => {
            format!("Edit: {}", file.unwrap_or("unknown file"))
        }
        "delete_file" => {
            format!("Delete: {}", file.unwrap_or("unknown file"))
        }
        _ => format!("Execute: {}", tool),
    }
}

pub(crate) fn is_parallel_safe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file" | "read_files" | "list_dir" | "directory_tree" | "get_file_info"
        | "search_in_dir" | "search_files" | "glob" | "analyze_project_structure"
        | "web_search" | "web_fetch"
        | "read_knowledge"
        | "list_builtin_skills" | "list_user_skills" | "match_skills"
        | "mcp_reload"
        | "job_output" | "list_jobs"
        | "get_env_info"
        | "run_background" | "spawn_process"
        | "read_lints"
        | "run_command" // parallel-safe: each command runs in its own process;
                         // concurrency guard is the session PIDs registry
    ) || is_mcp_tool(tool_name)
}

/// MCP tools are assumed parallel-safe (they're typically read-only API calls).
fn is_mcp_tool(tool_name: &str) -> bool {
    tool_name.starts_with("mcp__") || tool_name.starts_with("mcp_")
}

/// Attempt to repair truncated JSON from the model by balancing braces,
/// closing open strings, and removing trailing commas.
pub(crate) fn repair_truncated_json(json_str: &str) -> String {
    let mut s = json_str.trim().to_string();

    // Count unclosed structures
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for c in s.chars() {
        if escaped { escaped = false; continue; }
        match c {
            '\\' => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => brace_depth += 1,
            '}' if !in_string => brace_depth -= 1,
            '[' if !in_string => bracket_depth += 1,
            ']' if !in_string => bracket_depth -= 1,
            _ => {}
        }
    }

    // Close any unclosed string
    if in_string {
        s.push('"');
    }

    // Remove trailing commas
    while s.ends_with(',') || s.ends_with(",\n") {
        s.pop();
        s = s.trim_end().to_string();
    }

    // Close unclosed brackets and braces
    for _ in 0..bracket_depth.max(0) {
        s.push(']');
    }
    for _ in 0..brace_depth.max(0) {
        s.push('}');
    }

    // If still not valid JSON, try to wrap in {}
    if serde_json::from_str::<serde_json::Value>(&s).is_err() {
        // Last resort: wrap as object
        if !s.starts_with('{') {
            s = format!("{{{}}}", s);
        }
    }

    s
}

pub(crate) fn make_tool(name: &str, desc: &str, schema: serde_json::Value) -> serde_json::Value {
    json!({"name": name, "description": desc, "input_schema": schema})
}

pub(crate) fn schema_obj(props: serde_json::Value, required: &[&str]) -> serde_json::Value {
    let mut s = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        s["required"] = json!(required);
    }
    s
}

pub(crate) async fn tool_read_lints(
    params: &serde_json::Value,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let path: Option<String> = params["path"].as_str().map(normalize_file_path);

    tokio::task::spawn_blocking(move || {
        let workspace = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT workspace FROM app_config", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .filter(|w: &String| !w.is_empty())
        };

        let result = if let Some(ref ws) = workspace {
            let mut mgr_guard = lsp_manager.lock().unwrap();
            let needs_init = mgr_guard.is_none();
            if needs_init {
                *mgr_guard = Some(LspManager::new(ws));
            }
            if let Some(ref mgr) = *mgr_guard {
                let diags = mgr.read_lints(path.as_deref());
                format_lints_result(&diags)
            } else {
                json!({"success": false, "error": "Failed to initialize LSP manager"}).to_string()
            }
        } else {
            json!({"success": false, "error": "No workspace set"}).to_string()
        };

        result
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

pub(crate) async fn tool_touch_file(
    params: &serde_json::Value,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let file_path: String = normalize_file_path(params["file_path"].as_str().unwrap_or(""));

    tokio::task::spawn_blocking(move || {
        let workspace = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT workspace FROM app_config", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .filter(|w: &String| !w.is_empty())
        };

        if let Some(ref ws) = workspace {
            let mut mgr_guard = lsp_manager.lock().unwrap();
            if mgr_guard.is_none() {
                *mgr_guard = Some(LspManager::new(ws));
            }
            if let Some(ref mut mgr) = *mgr_guard {
                match mgr.touch_file(&file_path) {
                    Ok(diags) => {
                        if diags.is_empty() {
                            json!({"success": true, "diagnostics": []}).to_string()
                        } else {
                            json!({"success": true, "diagnostics": diags}).to_string()
                        }
                    }
                    Err(e) => json!({"success": false, "error": e}).to_string(),
                }
            } else {
                json!({"success": false, "error": "LSP manager init failed"}).to_string()
            }
        } else {
            json!({"success": false, "error": "No workspace set"}).to_string()
        }
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

pub(crate) async fn tool_ask_choice(
    params: &serde_json::Value,
    session_id: i64,
    agent_type: &str,
    pending_asks: PendingAsks,
    app_handle: AppHandle,
) -> String {
    let questions: serde_json::Value = params.get("questions").cloned().unwrap_or(json!([]));
    let ask_id = format!("ask_{}_{}", session_id, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let (tx, rx) = tokio::sync::oneshot::channel();
    pending_asks.lock().unwrap().insert(ask_id.clone(), tx);

    let _ = app_handle.emit("ask_choice", json!({
        "id": ask_id,
        "session_id": session_id,
        "agent_type": agent_type,
        "questions": questions
    }));

    // Wait up to 10 minutes for user response, then emit a timeout/expired event
    // so the frontend can dismiss the dialog gracefully.
    match tokio::time::timeout(std::time::Duration::from_secs(ASK_CHOICE_TIMEOUT_SECS), rx).await {
        Ok(Ok(answers)) => answers,
        Ok(Err(_)) => {
            // sender dropped = dialog closed / cancelled by frontend
            json!({"cancelled": true}).to_string()
        }
        Err(_) => {
            // 10-minute timeout: remove the stale sender so respond_ask can't crash on it
            pending_asks.lock().unwrap().remove(&ask_id);
            let _ = app_handle.emit("ask_choice_timeout", json!({"id": ask_id}));
            json!({"timeout": true, "message": "用户未在 10 分钟内回应，已超时"}).to_string()
        }
    }
}

pub(crate) fn format_lints_result(diags: &[crate::lsp_types::FileDiagnostics]) -> String {
    if diags.is_empty() || diags.iter().all(|d| d.diagnostics.is_empty()) {
        return json!({"success": true, "message": "No linter errors found"}).to_string();
    }

    let mut output = String::from("Linter diagnostics:\n\n");
    for file_diag in diags {
        if file_diag.diagnostics.is_empty() { continue; }
        output.push_str(&format!("## {}\n", file_diag.file));
        output.push_str(&format_diagnostics(&file_diag.diagnostics));
        output.push_str("\n\n");
    }

    json!({"success": true, "diagnostics": output}).to_string()
}

// ========== Todo Tracking ==========

pub(crate) async fn tool_todo_write(
    params: &serde_json::Value,
    todo_store: &Arc<StdMutex<HashMap<i64, String>>>,
    session_id: i64,
) -> String {
    let todos = match params["todos"].as_array() {
        Some(arr) => arr,
        None => return json!({"error": "todos array required"}).to_string(),
    };

    // Validate each item
    for (i, item) in todos.iter().enumerate() {
        if item["content"].as_str().is_none() || item["content"].as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
            return json!({"error": format!("todo[{}]: content is required", i)}).to_string();
        }
        let status = item["status"].as_str().unwrap_or("");
        if !["pending", "in_progress", "completed"].contains(&status) {
            return json!({"error": format!("todo[{}]: status must be pending|in_progress|completed, got '{}'", i, status)}).to_string();
        }
    }

    // Count statuses for summary
    let mut pending = 0i32;
    let mut in_progress = 0i32;
    let mut completed = 0i32;
    for item in todos {
        match item["status"].as_str().unwrap_or("") {
            "pending" => pending += 1,
            "in_progress" => in_progress += 1,
            "completed" => completed += 1,
            _ => {}
        }
    }

    let total = todos.len();
    let todos_json = serde_json::to_string(&params).unwrap_or_default();

    if let Ok(mut store) = todo_store.lock() {
        store.insert(session_id, todos_json);
    }

    json!({
        "todos": todos,
        "summary": format!("{} 项: {} 待处理, {} 进行中, {} 已完成", total, pending, in_progress, completed),
        "pct": if total > 0 { (completed as f64 / total as f64 * 100.0) as i32 } else { 0 }
    }).to_string()
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    // --- compute_diff ---

    #[test]
    fn diff_single_line_change() {
        let old = "line1\nline2\nline3";
        let new = "line1\nline2_changed\nline3";
        let diff = compute_diff(old, new);
        assert!(diff.contains("@@"), "diff should have @@ header");
        assert!(diff.contains("-line2"), "diff should show removed line");
        assert!(diff.contains("+line2_changed"), "diff should show added line");
    }

    #[test]
    fn diff_no_change() {
        let old = "line1\nline2";
        let new = "line1\nline2";
        let diff = compute_diff(old, new);
        // When there's no actual change, diff should be minimal
        assert!(diff.contains("@@"));
    }

    #[test]
    fn diff_empty_to_content() {
        let old = "";
        let new = "hello world";
        let diff = compute_diff(old, new);
        assert!(diff.contains("+hello world"));
    }

    #[test]
    fn diff_content_to_empty() {
        let old = "hello world";
        let new = "";
        let diff = compute_diff(old, new);
        assert!(diff.contains("-hello world"));
    }

    #[test]
    fn diff_multiple_lines() {
        let old = "a\nb\nc\nd\ne";
        let new = "a\nx\ny\nd\ne";
        let diff = compute_diff(old, new);
        assert!(diff.contains("-b"));
        assert!(diff.contains("-c"));
        assert!(diff.contains("+x"));
        assert!(diff.contains("+y"));
        // a, d, e should not appear in diff (unchanged context)
        // They might appear as context lines though, so we don't assert absence
    }

    // --- count_chars ---

    #[test]
    fn chars_growth() {
        let s = count_chars("abc", "abcdef");
        assert!(s.contains("+3"), "expected +3, got: {}", s);
    }

    #[test]
    fn chars_shrink() {
        let s = count_chars("abcdef", "abc");
        assert!(s.contains("-3"), "expected -3, got: {}", s);
    }

    #[test]
    fn chars_same_length() {
        let s = count_chars("abc", "xyz");
        assert!(s.contains("3->3"), "unexpected: {}", s);
    }

    // --- truncate_str ---

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate_str("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    // --- normalize_file_path ---

    #[test]
    fn normalize_removes_quotes() {
        let result = normalize_file_path("\"C:/test/path\"");
        assert!(!result.contains('"'));
    }

    #[test]
    fn normalize_preserves_valid_path() {
        let result = normalize_file_path("/tmp/test");
        assert!(!result.is_empty());
    }

    // --- glob_to_regex ---

    #[test]
    fn glob_wildcard_to_dot_star() {
        let re = glob_to_regex("*.rs");
        assert_eq!(re, "^.*\\.rs$");
    }

    #[test]
    fn glob_files_with_path() {
        let re = glob_to_regex("src/**/*.ts");
        assert!(re.contains(".*"));
        assert!(re.starts_with("^"));
        assert!(re.ends_with("$"));
    }

    #[test]
    fn glob_special_chars_escaped() {
        let re = glob_to_regex("test.+");
        assert!(re.contains("\\."));  // dot should be escaped
        assert!(re.contains("\\+"));  // plus should be escaped
    }

    // --- base64 ---

    #[test]
    fn base64_roundtrip_ascii() {
        let input = b"Hello World";
        let encoded = base64_encode(input);
        assert!(!encoded.is_empty());
        // Standard base64: SGVsbG8gV29ybGQ=
        assert_eq!(encoded, "SGVsbG8gV29ybGQ=");
    }

    #[test]
    fn base64_empty_input() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn base64_single_byte() {
        let encoded = base64_encode(b"A");
        assert_eq!(encoded, "QQ==");
    }

    #[test]
    fn base64_two_bytes() {
        let encoded = base64_encode(b"AB");
        assert_eq!(encoded, "QUI=");
    }

    // --- binary detection ---

    #[test]
    fn detect_binary_null_byte() {
        // Create a temp file with a null byte
        let dir = std::env::temp_dir();
        let path = dir.join("test_binary.bin");
        std::fs::write(&path, b"hello\0world").unwrap();
        assert!(is_binary_file(&path.to_string_lossy()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn text_file_not_binary() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_text.txt");
        std::fs::write(&path, b"hello world").unwrap();
        assert!(!is_binary_file(&path.to_string_lossy()));
        let _ = std::fs::remove_file(&path);
    }

    // --- line counting ---

    #[test]
    fn count_lines_empty_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_empty.txt");
        std::fs::write(&path, "").unwrap();
        assert_eq!(count_lines(&path.to_string_lossy()), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn count_lines_multiline() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_lines.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();
        assert_eq!(count_lines(&path.to_string_lossy()), 3);
        let _ = std::fs::remove_file(&path);
    }
}

// ==================== Command execution tests ====================

#[cfg(test)]
mod cmd_tests {
    use super::*;

    // --- parse_command ---

    #[test]
    fn parse_simple_two_words() {
        let (prog, args) = parse_command("git status");
        assert_eq!(prog, "git");
        assert_eq!(args, vec!["status"]);
    }

    #[test]
    fn parse_single_program() {
        let (prog, args) = parse_command("dir");
        assert_eq!(prog, "dir");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_with_double_quotes() {
        let (prog, args) = parse_command(r#"cmd /c "echo hello world""#);
        assert_eq!(prog, "cmd");
        assert_eq!(args, vec!["/c", "echo hello world"]);
    }

    #[test]
    fn parse_with_single_quotes() {
        let (prog, args) = parse_command("echo 'hello world'");
        assert_eq!(prog, "echo");
        assert_eq!(args, vec!["hello world"]);
    }

    #[test]
    fn parse_nested_quotes() {
        let (prog, args) = parse_command(r#"node -e "console.log('hello > world')""#);
        assert_eq!(prog, "node");
        assert_eq!(args, vec!["-e", "console.log('hello > world')"]);
    }

    #[test]
    fn parse_preserves_metacharacters_in_quotes() {
        let (prog, args) = parse_command(r#"echo "hello | world""#);
        assert_eq!(prog, "echo");
        assert_eq!(args, vec!["hello | world"]);
    }

    #[test]
    fn parse_empty_string() {
        let (prog, args) = parse_command("");
        assert!(prog.is_empty());
        assert!(args.is_empty());
    }

    #[test]
    fn parse_whitespace_only() {
        let (prog, _) = parse_command("   \t  ");
        assert!(prog.is_empty());
    }

    #[test]
    fn parse_multiple_flags() {
        let (prog, args) = parse_command("cmd /d /s /c dir");
        assert_eq!(prog, "cmd");
        assert_eq!(args, vec!["/d", "/s", "/c", "dir"]);
    }

    #[test]
    fn parse_powershell_command() {
        let (prog, args) = parse_command(r#"powershell -Command "Get-Process | Sort-Object CPU""#);
        assert_eq!(prog, "powershell");
        assert_eq!(args, vec!["-Command", "Get-Process | Sort-Object CPU"]);
    }

    #[test]
    fn parse_path_with_spaces_quoted() {
        let (prog, args) = parse_command(r#""C:\Program Files\Git\bin\git.exe" status"#);
        assert_eq!(prog, "C:\\Program Files\\Git\\bin\\git.exe");
        assert_eq!(args, vec!["status"]);
    }

    #[test]
    fn parse_unquoted_metacharacters_kept_as_args() {
        let (prog, args) = parse_command("git diff > patch");
        assert_eq!(prog, "git");
        assert_eq!(args, vec!["diff", ">", "patch"]);
    }

    #[test]
    fn parse_trailing_spaces() {
        let (prog, args) = parse_command("echo hello   ");
        assert_eq!(prog, "echo");
        assert_eq!(args, vec!["hello"]);
    }

    #[test]
    fn parse_leading_spaces() {
        let (prog, args) = parse_command("   echo hello");
        assert_eq!(prog, "echo");
        assert_eq!(args, vec!["hello"]);
    }

    #[test]
    fn parse_tab_separator() {
        let (prog, args) = parse_command("git\tstatus");
        assert_eq!(prog, "git");
        assert_eq!(args, vec!["status"]);
    }

    // --- has_shell_metacharacters ---

    #[test]
    fn no_metacharacters_simple() {
        assert!(!has_shell_metacharacters("git status"));
        assert!(!has_shell_metacharacters("npm install"));
        assert!(!has_shell_metacharacters("node script.js"));
    }

    #[test]
    fn detects_unquoted_pipe() {
        assert!(has_shell_metacharacters("dir | findstr test"));
    }

    #[test]
    fn detects_unquoted_redirect() {
        assert!(has_shell_metacharacters("git diff > patch"));
        assert!(has_shell_metacharacters("echo hello >> log.txt"));
    }

    #[test]
    fn detects_unquoted_input_redirect() {
        assert!(has_shell_metacharacters("mysql < dump.sql"));
    }

    #[test]
    fn detects_unquoted_and_and() {
        assert!(has_shell_metacharacters("npm install && npm run build"));
    }

    #[test]
    fn ignores_pipe_in_double_quotes() {
        assert!(!has_shell_metacharacters(r#"echo "hello | world""#));
    }

    #[test]
    fn ignores_pipe_in_single_quotes() {
        assert!(!has_shell_metacharacters("echo 'hello | world'"));
    }

    #[test]
    fn ignores_redirect_in_quotes() {
        assert!(!has_shell_metacharacters(r#"cmd /c "echo hello > file.txt""#));
    }

    #[test]
    fn ignores_and_and_in_quotes() {
        assert!(!has_shell_metacharacters(r#"echo "install && build""#));
    }

    #[test]
    fn detects_gt_in_single_quotes_outside_double() {
        // ' outside double quotes: > is outside quotes
        assert!(has_shell_metacharacters("echo 'hello' > file"));
    }

    #[test]
    fn mixed_quote_metachar_safe() {
        // Pipe inside single quotes inside double quotes → safe
        assert!(!has_shell_metacharacters(r#"python -c "print('hello | world')""#));
    }

    #[test]
    fn nested_pipe_safe_in_powershell() {
        // The | is inside "..." so it's safe for our detector
        assert!(!has_shell_metacharacters(r#"powershell -Command "Get-Process | Sort CPU""#));
    }

    #[test]
    fn bare_pipe_after_shell_wrap() {
        // cmd /c ... | findstr → | is outside quotes and after cmd args
        assert!(has_shell_metacharacters("cmd /c dir | findstr test"));
    }

    #[test]
    fn no_false_positive_on_single_ampersand() {
        // Single & is not flagged (only && is)
        assert!(!has_shell_metacharacters("echo hello & echo world"));
    }

    // --- is_shell_program ---

    #[test]
    fn windows_shells_recognized() {
        assert!(is_shell_program("cmd"));
        assert!(is_shell_program("cmd.exe"));
        assert!(is_shell_program("powershell"));
        assert!(is_shell_program("powershell.exe"));
        assert!(is_shell_program("pwsh"));
        assert!(is_shell_program("pwsh.exe"));
    }

    #[test]
    fn unix_shells_recognized() {
        assert!(is_shell_program("sh"));
        assert!(is_shell_program("bash"));
        assert!(is_shell_program("zsh"));
        assert!(is_shell_program("fish"));
        assert!(is_shell_program("dash"));
    }

    #[test]
    fn full_path_shell() {
        assert!(is_shell_program("/bin/bash"));
        assert!(is_shell_program("/usr/bin/sh"));
        assert!(is_shell_program("C:\\Windows\\System32\\cmd.exe"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_shell_program("CMD"));
        assert!(is_shell_program("PowerShell"));
        assert!(is_shell_program("Bash"));
    }

    #[test]
    fn wsl_recognized() {
        assert!(is_shell_program("wsl"));
    }

    #[test]
    fn non_shell_programs() {
        assert!(!is_shell_program("git"));
        assert!(!is_shell_program("npm"));
        assert!(!is_shell_program("node"));
        assert!(!is_shell_program("python"));
        assert!(!is_shell_program("echo"));
        assert!(!is_shell_program("dir"));
    }

    // --- truncate_tool_result ---

    #[test]
    fn truncate_short_output_unchanged() {
        let s = "hello world".to_string();
        assert_eq!(truncate_tool_result(s.clone()), s);
    }

    #[test]
    fn truncate_long_output() {
        // 5000 lines × 20 bytes each = 100,000+ bytes
        // This ensures both bytes AND lines exceed the cap
        let input = (0..5000).map(|i| format!("line{:04}abcdefghijklm", i)).collect::<Vec<_>>().join("\n");
        assert!(input.len() > 55_000, "input should be > 50KB cap");
        assert!(input.lines().count() > 2500, "input should be > 2000 line cap");
        let result = truncate_tool_result(input);
        assert!(result.len() < 60_000, "result {} should be much smaller than input", result.len());
        assert!(result.contains("truncated"), "should contain truncated marker, got first 200 chars: {}", &result[..result.len().min(200)]);
    }

    #[test]
    fn truncate_very_long_single_line() {
        let s = "x".repeat(60_000);
        // 60K chars is > 50KB MAX_BYTES, should trigger truncation
        let result = truncate_tool_result(s);
        // After truncation: head (85% of 2000 lines = 1700 chars) + marker + tail
        assert!(result.contains("truncated"), "single long line should be truncated, got length {}", result.len());
    }

    #[test]
    fn truncate_preserves_head_and_tail() {
        let lines: Vec<String> = (0..3000).map(|i| format!("line{}", i)).collect();
        let input = lines.join("\n");
        let result = truncate_tool_result(input);
        assert!(result.contains("line0"));
        assert!(result.contains("line2999"));
    }

    #[test]
    fn truncate_exact_boundary() {
        // Exactly at 2000 lines — no truncation
        let lines: Vec<String> = (0..2000).map(|i| format!("L{}", i)).collect();
        let input = lines.join("\n");
        let result = truncate_tool_result(input);
        assert!(result.contains("L1999"), "tail should be present: {}", result.lines().count());
    }

    // --- is_parallel_safe ---

    #[test]
    fn read_only_tools_parallel_safe() {
        assert!(is_parallel_safe("read_file"));
        assert!(is_parallel_safe("read_files"));
        assert!(is_parallel_safe("list_dir"));
        assert!(is_parallel_safe("directory_tree"));
        assert!(is_parallel_safe("get_file_info"));
        assert!(is_parallel_safe("search_in_dir"));
        assert!(is_parallel_safe("search_files"));
        assert!(is_parallel_safe("glob"));
        assert!(is_parallel_safe("analyze_project_structure"));
    }

    #[test]
    fn web_tools_parallel_safe() {
        assert!(is_parallel_safe("web_search"));
        assert!(is_parallel_safe("web_fetch"));
    }

    #[test]
    fn command_tools_parallel_safe() {
        assert!(is_parallel_safe("run_command"));
        assert!(is_parallel_safe("run_background"));
        assert!(is_parallel_safe("spawn_process"));
    }

    #[test]
    fn write_tools_not_parallel_safe() {
        assert!(!is_parallel_safe("write_file"));
        assert!(!is_parallel_safe("edit_file"));
        assert!(!is_parallel_safe("multi_edit"));
        assert!(!is_parallel_safe("delete_file"));
    }
}

// ==================== Shell routing integration tests ====================

#[cfg(test)]
mod shell_routing_tests {
    use super::*;

    /// Simulates what tool_run_command does for shell routing,
    /// without actually spawning processes.
    fn route_command(command: &str) -> &'static str {
        if command.is_empty() {
            return "empty";
        }
        let (program, _) = parse_command(command);
        if program.is_empty() {
            return "parse_error";
        }
        let needs_shell = has_shell_metacharacters(command);
        if needs_shell && !is_shell_program(&program) {
            return "system_shell";   // Path 1: wrap in cmd/sh
        }
        if is_shell_program(&program) {
            return "direct_shell";   // Path 2: explicit shell, run as-is
        }
        // Path 3: try direct, fallback to shell on error
        // We can't test runtime behavior, but we verify the routing intent
        "direct_with_fallback"
    }

    #[test]
    fn shell_builtin_fallback() {
        // These would fail directly, then succeed via shell fallback
        assert_eq!(route_command("echo hello"), "direct_with_fallback");
        assert_eq!(route_command("dir"), "direct_with_fallback");
        assert_eq!(route_command("type file.txt"), "direct_with_fallback");
    }

    #[test]
    fn simple_commands_direct() {
        assert_eq!(route_command("git status"), "direct_with_fallback");
        assert_eq!(route_command("npm install"), "direct_with_fallback");
        assert_eq!(route_command("python script.py"), "direct_with_fallback");
    }

    #[test]
    fn redirect_forces_system_shell() {
        assert_eq!(route_command("git diff > patch"), "system_shell");
        assert_eq!(route_command("echo hello >> log.txt"), "system_shell");
    }

    #[test]
    fn pipe_forces_system_shell() {
        assert_eq!(route_command("dir | findstr test"), "system_shell");
    }

    #[test]
    fn chaining_forces_system_shell() {
        assert_eq!(route_command("npm install && npm run build"), "system_shell");
    }

    #[test]
    fn input_redirect_forces_system_shell() {
        assert_eq!(route_command("mysql < dump.sql"), "system_shell");
    }

    #[test]
    fn explicit_cmd_runs_direct() {
        assert_eq!(route_command("cmd /c dir"), "direct_shell");
        assert_eq!(route_command("cmd /d /s /c \"echo hello\""), "direct_shell");
    }

    #[test]
    fn explicit_powershell_runs_direct() {
        assert_eq!(route_command("powershell -Command Get-Date"), "direct_shell");
        assert_eq!(route_command("pwsh -c Write-Host hello"), "direct_shell");
    }

    #[test]
    fn explicit_bash_runs_direct() {
        assert_eq!(route_command("bash -c 'ls -la'"), "direct_shell");
        assert_eq!(route_command("sh script.sh"), "direct_shell");
    }

    #[test]
    fn quoted_metacharacters_direct_execution() {
        // These have metacharacters inside quotes → not flagged → try direct
        assert_eq!(route_command(r#"echo "hello > world""#), "direct_with_fallback");
        assert_eq!(route_command(r#"python -c "print('a | b')""#), "direct_with_fallback");
    }

    #[test]
    fn empty_command_returns_empty() {
        assert_eq!(route_command(""), "empty");
    }

    #[test]
    fn var_expansion_fallback() {
        // %VAR% not detected as metacharacter → falls back on spawn error
        assert_eq!(route_command("echo %PATH%"), "direct_with_fallback");
    }
}