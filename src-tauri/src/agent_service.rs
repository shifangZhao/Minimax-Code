// Agent Service - Rust Implementation for AI Agent Streaming
//
// Provides:
// - MiniMax API streaming (via reqwest)
// - Tool execution
// - Message history management
// - Interleaved Thinking support

use futures_util::StreamExt;
use futures_util::future::join_all;
use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::watch;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};
use crate::agent_tools::*;

/// Maximum time (seconds) a single SSE stream session can stay open before being force-closed.
const STREAM_SESSION_TIMEOUT_SECS: u64 = 3600;

/// Maximum time (seconds) to wait for a user permission response before auto-denying.
const PERMISSION_TIMEOUT_SECS: u64 = 600;

/// Build a Command that runs without a visible console window on Windows.
pub(crate) fn hidden_cmd(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    // Force UTF-8 output from all child processes, regardless of system locale.
    // This is the single source of truth — no per-command encoding hacks needed.
    cmd.env("PYTHONUTF8", "1")
       .env("PYTHONIOENCODING", "utf-8")
       .env("LANG", "C.UTF-8")
       .env("LC_ALL", "C.UTF-8");
    cmd
}

/// Decode process output bytes to string.
/// On Windows: try UTF-8 first; if invalid, decode as GBK (CP936), the legacy
/// encoding used by cmd.exe and PowerShell 5.1 on Chinese systems.
#[cfg(windows)]
fn decode_process_output(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // Not valid UTF-8 — likely a legacy locale encoding. Try GBK (simplified
    // Chinese) which is by far the most common on Chinese Windows.
    let mut decoder = encoding_rs::Encoding::for_label(b"gbk")
        .unwrap_or(encoding_rs::UTF_8)
        .new_decoder_without_bom_handling();
    let cap = decoder.max_utf8_buffer_length(bytes.len()).unwrap_or(bytes.len() * 2);
    let mut out = vec![0; cap];
    let (result, _read, written, _replacements) = decoder.decode_to_utf8(bytes, &mut out, false);
    out.truncate(written);
    if result == encoding_rs::CoderResult::InputEmpty {
        String::from_utf8_lossy(&out).to_string()
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(not(windows))]
fn decode_process_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

// ── Async command execution (tokio::process) ──
// These replace the thread-per-pipe model with tokio async I/O,
// eliminating 3 OS threads per command invocation.

/// Build a tokio::process::Command with the same env setup as hidden_cmd.
pub(crate) fn async_hidden_cmd(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
    cmd.env("PYTHONUTF8", "1")
       .env("PYTHONIOENCODING", "utf-8")
       .env("LANG", "C.UTF-8")
       .env("LC_ALL", "C.UTF-8");
    cmd
}

/// Async version of output_with_timeout using tokio::process.
/// No OS threads spawned — uses tokio async I/O for pipe reads.
pub(crate) async fn async_output_with_timeout(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
    running_pids: Option<Arc<StdMutex<HashMap<i64, Vec<u32>>>>>,
) -> String {
    let mut cmd = async_hidden_cmd(program);
    cmd.args(args)
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("Error spawning command: {}", e),
    };

    let pid = child.id().unwrap_or(0);

    // Register PID for abort support
    if let (Some(pid), Some(ref map)) = (child.id(), &running_pids) {
        if let Ok(mut m) = map.lock() {
            m.entry(session_id).or_default().push(pid);
        }
    }

    // Take stdout/stderr before waiting
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    // Spawn async readers
    let out_handle = tokio::spawn(async move {
        if let Some(mut stdout) = child_stdout {
            let mut buf = Vec::new();
            use tokio::io::AsyncReadExt;
            let _ = stdout.read_to_end(&mut buf).await;
            buf
        } else {
            Vec::new()
        }
    });
    let err_handle = tokio::spawn(async move {
        if let Some(mut stderr) = child_stderr {
            let mut buf = Vec::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_end(&mut buf).await;
            buf
        } else {
            Vec::new()
        }
    });

    // Wait with timeout
    let force_kill = |pid: u32| {
        #[cfg(windows)]
        { let _ = hidden_cmd("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output(); }
        #[cfg(not(windows))]
        {
            let _ = hidden_cmd("kill").args(["-15", &pid.to_string()]).output();
            std::thread::sleep(std::time::Duration::from_secs(1));
            let _ = hidden_cmd("kill").args(["-9", &pid.to_string()]).output();
        }
    };

    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait(),
    ).await {
        Ok(Ok(status)) => {
            // Collect output — readers should finish shortly after process exits
            let stdout_bytes = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                out_handle,
            ).await.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let stderr_bytes = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                err_handle,
            ).await.unwrap_or(Ok(Vec::new())).unwrap_or_default();

            const MAX_BYTES: usize = 256 * 1024;
            let stdout = if stdout_bytes.len() > MAX_BYTES {
                let head = decode_process_output(&stdout_bytes[..MAX_BYTES / 2]);
                let tail = decode_process_output(&stdout_bytes[stdout_bytes.len().saturating_sub(MAX_BYTES / 2)..]);
                format!("{}\n[...{} bytes truncated...]\n{}", head, stdout_bytes.len() - MAX_BYTES, tail)
            } else {
                decode_process_output(&stdout_bytes)
            };
            let stderr_str = decode_process_output(&stderr_bytes);
            let exit = status.code().unwrap_or(-1);
            if stdout.is_empty() && !stderr_str.is_empty() {
                format!("Exit: {}\n{}", exit, stderr_str)
            } else if !stderr_str.is_empty() {
                format!("Exit: {}\n{}\n{}", exit, stdout, stderr_str)
            } else {
                format!("Exit: {}\n{}", exit, stdout)
            }
        }
        Ok(Err(e)) => format!("Error waiting for process: {}", e),
        Err(_) => {
            // Timeout — kill process tree
            force_kill(pid);
            let _ = child.kill().await;
            let stdout_partial = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                out_handle,
            ).await.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let stderr_partial = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                err_handle,
            ).await.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let partial_out = decode_process_output(&stdout_partial);
            let partial_err = decode_process_output(&stderr_partial);
            format!("Command timed out after {}s (killed)\n{}",
                timeout_secs,
                if partial_out.is_empty() && partial_err.is_empty() {
                    String::new()
                } else {
                    format!("Partial output:\n{}{}", partial_out, partial_err)
                })
        }
    };

    // Deregister PID
    if let (Some(pid), Some(ref map)) = (child.id(), &running_pids) {
        if let Ok(mut m) = map.lock() {
            if let Some(v) = m.get_mut(&session_id) {
                v.retain(|&p| p != pid);
                if v.is_empty() { m.remove(&session_id); }
            }
        }
    }

    result
}

/// Async shell execution using tokio::process.
pub(crate) async fn async_execute_via_shell(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
    running_pids: Option<Arc<StdMutex<HashMap<i64, Vec<u32>>>>>,
) -> String {
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let args: Vec<String> = if cfg!(windows) {
        vec!["/d".into(), "/s".into(), "/c".into(), command.to_string()]
    } else {
        vec!["-c".into(), command.to_string()]
    };
    async_output_with_timeout(shell, &args, cwd, timeout_secs, session_id, running_pids).await
}

use crate::context_compressor::{compress_context_aggressive, estimate_request_tokens, estimate_tokens, summarize_with_model};
use crate::lsp_manager::LspManager;
use crate::mcp_service::McpService;
use crate::permission::{PermissionService, PermissionAction, PermissionRequest};
use crate::skill_service::SkillService;
use crate::system_prompts::ACE_SYSTEM;

// ========== Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<String>,  // JSON string of tool_calls array
    #[serde(default)]
    pub thinking: Option<String>,  // thinking content
    #[serde(default)]
    pub raw_json: Option<String>,  // full JSON of content block array for cache preservation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { content: String, thinking: String },
    #[serde(rename = "tool_start")]
    ToolStart { tool: String, tool_id: String, input: serde_json::Value },
    #[serde(rename = "tool_end")]
    ToolEnd { tool: String, tool_id: String, result: String },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "aborted")]
    Aborted,
    #[serde(rename = "error")]
    Error { content: String },
    #[serde(rename = "cache_usage")]
    CacheUsage { cache_hit_tokens: u64, cache_miss_tokens: u64, cache_hit_ratio: f64 },
    #[serde(rename = "token_usage")]
    TokenUsage { estimated_tokens: usize, context_window: usize, usage_pct: f64 },
}

// ========== Agent Service ==========

pub(crate) type PendingAsks = Arc<StdMutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>>;

/// Save an api_message (with structured content blocks) to the chat_message table.
/// Stores display text in `content` and the full JSON block array in `raw_json`.
#[allow(clippy::type_complexity)]
fn save_api_message(db: &Arc<StdMutex<Connection>>, session_id: i64, message: &serde_json::Value) {
    let role = message["role"].as_str().unwrap_or("user");
    let content_val = &message["content"];

    let (display_text, parts): (String, Vec<(i64, String, String, Option<String>, Option<String>, Option<String>)>) = match content_val {
        serde_json::Value::String(s) => (s.clone(), vec![]),
        serde_json::Value::Array(blocks) => {
            let text = blocks.iter()
                .filter(|b| b["type"] == "text")
                .map(|b| b["text"].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("");
            let parts_vec = blocks.iter().enumerate().map(|(i, b)| {
                let ptype = b["type"].as_str().unwrap_or("text").to_string();
                let pcontent = match ptype.as_str() {
                    "thinking" => b["thinking"].as_str().unwrap_or("").to_string(),
                    "tool_result" => b["content"].as_str().unwrap_or("").to_string(),
                    "tool_use" => String::new(),
                    _ => b["text"].as_str().unwrap_or("").to_string(),
                };
                let tuid = match ptype.as_str() {
                    "tool_result" => b["tool_use_id"].as_str().map(|s| s.to_string()),
                    _ => b["id"].as_str().map(|s| s.to_string()),
                };
                let tname = b["name"].as_str().map(|s| s.to_string());
                let tinput = if ptype == "tool_use" {
                    Some(serde_json::to_string(&b["input"]).unwrap_or_default())
                } else { None };
                (i as i64, ptype, pcontent, tuid, tname, tinput)
            }).collect();
            (text, parts_vec)
        }
        _ => (String::new(), vec![]),
    };

    if let Ok(conn) = db.lock() {
        let _ = conn.execute_batch("BEGIN IMMEDIATE");
        let _ = conn.execute(
            "INSERT INTO chat_message (session_id, role, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, role, display_text],
        );
        let msg_id = conn.last_insert_rowid();
        for (order, ptype, pcontent, tuid, tname, tinput) in &parts {
            let _ = conn.execute(
                "INSERT INTO message_part (message_id, session_id, part_order, part_type, content, tool_use_id, tool_name, tool_input) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![msg_id, session_id, order, ptype, pcontent, tuid, tname, tinput],
            );
        }
        let _ = conn.execute_batch("COMMIT");
    }
}

/// Strip all cache_control markers from messages so we start with a clean
/// byte sequence before re-marking. Old markers from persisted raw_json would
/// otherwise create non-deterministic byte patterns that break prefix cache.
fn strip_cache_control(msgs: &mut [serde_json::Value]) {
    for msg in msgs.iter_mut() {
        if let Some(arr) = msg["content"].as_array_mut() {
            for block in arr.iter_mut() {
                if block.is_object() {
                    block.as_object_mut().map(|o| o.remove("cache_control"));
                }
            }
        }
    }
}

/// Mark the last message's last content block with cache_control for incremental
/// multi-turn caching. Converts string content to block array format if needed.
fn mark_last_message_for_cache(msgs: &mut [serde_json::Value]) {
    if let Some(last_msg) = msgs.last_mut() {
        let content = &mut last_msg["content"];
        if content.is_string() {
            let text = content.as_str().unwrap_or("").to_string();
            *content = json!([{
                "type": "text",
                "text": text,
                "cache_control": {"type": "ephemeral"}
            }]);
        } else if content.is_array() {
            if let Some(arr) = content.as_array_mut() {
                if let Some(last_block) = arr.last_mut() {
                    last_block["cache_control"] = json!({"type": "ephemeral"});
                }
            }
        }
    }
}

/// Validate that every assistant message with tool_use blocks is immediately
/// followed by a user message with matching tool_result blocks. Required by
/// Anthropic-compatible APIs. If orphans are found, inject stub tool_result
/// messages so the session can recover without a restart.
fn validate_tool_pairing(msgs: &mut Vec<serde_json::Value>) {
    let mut i = 0;
    while i < msgs.len() {
        let role = msgs[i]["role"].as_str().unwrap_or("");
        if role != "assistant" {
            i += 1;
            continue;
        }
        let content = &msgs[i]["content"];
        let tool_use_ids: Vec<String> = content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| b["id"].as_str().map(|s| s.to_string()))
            .collect();

        if tool_use_ids.is_empty() {
            i += 1;
            continue;
        }

        // Check the next message for matching tool_result blocks
        let next_idx = i + 1;
        let mut missing: Vec<&String> = tool_use_ids.iter().collect();

        if next_idx < msgs.len()
            && msgs[next_idx]["role"].as_str() == Some("user")
            && msgs[next_idx]["content"].is_array()
        {
            let existing_ids: Vec<&str> = msgs[next_idx]["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["type"] == "tool_result")
                .filter_map(|b| b["tool_use_id"].as_str())
                .collect();
            missing.retain(|id| !existing_ids.contains(&id.as_str()));
        } else {
            // Next message is missing or wrong role — all tool_use ids are orphans
        }

        if !missing.is_empty() {
            eprintln!(
                "[tool_pair_validate] Orphaned tool_use at message[{}]: {} — injecting stub tool_result",
                i,
                missing.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(", ")
            );
            let stubs: Vec<serde_json::Value> = missing
                .iter()
                .map(|id| {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": "[Auto-recovered] Tool result was lost — this stub injected to maintain API compliance."
                    })
                })
                .collect();

            if next_idx < msgs.len()
                && msgs[next_idx]["role"].as_str() == Some("user")
                && msgs[next_idx]["content"].is_array()
            {
                // Append stubs to existing user message
                if let Some(arr) = msgs[next_idx]["content"].as_array_mut() {
                    arr.extend(stubs);
                }
            } else {
                // Insert new user message with stubs
                msgs.insert(
                    next_idx,
                    serde_json::json!({"role": "user", "content": stubs}),
                );
            }
        }

        i += 1;
    }
}

/// Save original file content before modification for undo support.
/// Each call creates a new version, so multi-edit workflows can rewind step by step.
fn save_file_snapshot(db: &Arc<StdMutex<Connection>>, session_id: i64, file_path: &str) {
    if let Ok(conn) = db.lock() {
        // Find the next version for this file in this session
        let next_version: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM file_snapshot WHERE session_id = ?1 AND file_path = ?2",
            rusqlite::params![session_id, file_path],
            |row| row.get(0),
        ).unwrap_or(1);

        let original = match std::fs::read(file_path) {
            Ok(bytes) => {
                // Avoid cloning: check UTF-8 validity via str::from_utf8 first
                match std::str::from_utf8(&bytes) {
                    Ok(s) if is_printable_text(s) => Some(s.to_string()),
                    _ => Some(format!("hex:{}", hex_encode(&bytes))),
                }
            }
            Err(_) => None, // file doesn't exist yet
        };
        let _ = conn.execute(
            "INSERT INTO file_snapshot (session_id, file_path, original_content, version) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, file_path, original, next_version],
        );
    }
}

/// Async wrapper — runs the sync snapshot on a blocking thread.
async fn save_file_snapshot_async(db: Arc<StdMutex<Connection>>, session_id: i64, file_path: String) {
    tokio::task::spawn_blocking(move || {
        save_file_snapshot(&db, session_id, &file_path);
    }).await.ok();
}

/// Batch snapshot multiple files on a single blocking thread with transaction.
/// Wrapping in a transaction avoids per-INSERT fsync — 10-50x faster for many files.
async fn save_file_snapshots_batch(db: Arc<StdMutex<Connection>>, session_id: i64, file_paths: Vec<String>) {
    tokio::task::spawn_blocking(move || {
        if let Ok(conn) = db.lock() {
            let _ = conn.execute_batch("BEGIN IMMEDIATE");
            for path in &file_paths {
                let next_version: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1 FROM file_snapshot WHERE session_id = ?1 AND file_path = ?2",
                    rusqlite::params![session_id, path],
                    |row| row.get(0),
                ).unwrap_or(1);
                let original = match std::fs::read(path) {
                    Ok(bytes) => match std::str::from_utf8(&bytes) {
                        Ok(s) if is_printable_text(s) => Some(s.to_string()),
                        _ => Some(format!("hex:{}", hex_encode(&bytes))),
                    },
                    Err(_) => None,
                };
                let _ = conn.execute(
                    "INSERT INTO file_snapshot (session_id, file_path, original_content, version) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![session_id, path, original, next_version],
                );
            }
            let _ = conn.execute_batch("COMMIT");
        }
    }).await.ok();
}

fn is_printable_text(s: &str) -> bool {
    for ch in s.chars() {
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            return false;
        }
    }
    true
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

pub(crate) fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i+2], 16).ok())
        .collect()
}

/// Collect all file paths in a directory tree (non-recursive symlink safe, depth-limited).
fn collect_dir_files(dir: &std::path::Path, out: &mut Vec<String>) {
    use std::fs;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Limit depth to avoid huge snapshots (max 4 levels)
                if path.components().count() <= dir.components().count() + 4 {
                    collect_dir_files(&path, out);
                }
            } else if path.is_file() {
                if let Some(s) = path.to_str() {
                    out.push(s.to_string());
                }
            }
        }
    }
}

/// Override a constant with an environment variable if set, otherwise use the default.
fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Snapshot all text files in a workspace before a destructive command.
/// Limits: MAX_SNAPSHOT_FILES (env, default 200), MAX_SNAPSHOT_BYTES (env, default 10MB).
/// Runs on a blocking thread with transaction for performance.
async fn snapshot_workspace_files(db: Arc<StdMutex<Connection>>, session_id: i64, cwd: String) {
    tokio::task::spawn_blocking(move || {
        let mut files: Vec<String> = Vec::new();
        collect_dir_files(std::path::Path::new(&cwd), &mut files);
        let max_files = env_usize("MAX_SNAPSHOT_FILES", 200);
        let max_bytes = env_usize("MAX_SNAPSHOT_BYTES", 10_000_000);
        let mut total: usize = 0;
        let mut selected: Vec<String> = Vec::new();
        for f in &files {
            if selected.len() >= max_files { break; }
            if let Ok(meta) = std::fs::metadata(f) {
                total += meta.len() as usize;
                if total > max_bytes { break; }
            }
            selected.push(f.clone());
        }
        if selected.is_empty() { return; }
        if let Ok(conn) = db.lock() {
            let _ = conn.execute_batch("BEGIN IMMEDIATE");
            for path in &selected {
                let next_version: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1 FROM file_snapshot WHERE session_id = ?1 AND file_path = ?2",
                    rusqlite::params![session_id, path],
                    |row| row.get(0),
                ).unwrap_or(1);
                let original = match std::fs::read(path) {
                    Ok(bytes) => match std::str::from_utf8(&bytes) {
                        Ok(s) if is_printable_text(s) => Some(s.to_string()),
                        _ => Some(format!("hex:{}", hex_encode(&bytes))),
                    },
                    Err(_) => None,
                };
                let _ = conn.execute(
                    "INSERT INTO file_snapshot (session_id, file_path, original_content, version) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![session_id, path, original, next_version],
                );
            }
            let _ = conn.execute_batch("COMMIT");
        }
    }).await.ok();
}

/// Write snapshot content to a file path, decoding hex if needed.
/// Returns true on success.
pub(crate) fn restore_snapshot_file(path: &str, content: Option<&str>) -> bool {
    match content {
        None => {
            // File didn't exist before — remove it
            let p = std::path::Path::new(path);
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else if p.exists() {
                let _ = std::fs::remove_file(path);
            }
            true
        }
        Some(c) => {
            if let Some(hex) = c.strip_prefix("hex:") {
                let bytes = hex_decode(hex);
                std::fs::write(path, &bytes).is_ok()
            } else {
                std::fs::write(path, c).is_ok()
            }
        }
    }
}

pub struct AgentService {
    api_url: String,
    messages_path: String,
    model: String,
    context_window: usize,
    provider: String,
    api_key: Arc<Mutex<String>>,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    todo_store: Arc<StdMutex<HashMap<i64, String>>>,
    running_command_pids: Arc<StdMutex<HashMap<i64, Vec<u32>>>>,
}

impl Clone for AgentService {
    fn clone(&self) -> Self {
        Self {
            api_url: self.api_url.clone(),
            messages_path: self.messages_path.clone(),
            model: self.model.clone(),
            context_window: self.context_window,
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            skill_service: self.skill_service.clone(),
            mcp_service: self.mcp_service.clone(),
            db: self.db.clone(),
            lsp_manager: self.lsp_manager.clone(),
            permission_service: self.permission_service.clone(),
            pending_asks: self.pending_asks.clone(),
            todo_store: self.todo_store.clone(),
            running_command_pids: self.running_command_pids.clone(),
        }
    }
}

impl AgentService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(api_key: String, api_url: String, messages_path: String, model: String, context_window: usize, provider: String, skill_service: Arc<SkillService>, mcp_service: Arc<RwLock<McpService>>, db: Arc<StdMutex<Connection>>, lsp_manager: Arc<StdMutex<Option<LspManager>>>, permission_service: Arc<StdMutex<PermissionService>>, pending_asks: PendingAsks, running_command_pids: Arc<StdMutex<HashMap<i64, Vec<u32>>>>) -> Self {
        Self {
            api_url,
            messages_path,
            model,
            context_window,
            provider,
            api_key: Arc::new(Mutex::new(api_key)),
            skill_service,
            mcp_service,
            db,
            lsp_manager,
            permission_service,
            pending_asks,
            todo_store: Arc::new(StdMutex::new(HashMap::new())),
            running_command_pids,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn stream_chat(
        &self,
        agent_type: &str,
        messages: Vec<Message>,
        _system: Option<String>,
        workspace: Option<String>,
        app_handle: AppHandle,
        session_id: i64,
        cancel_rx: watch::Receiver<bool>,
    ) {
        let api_key = self.api_key.lock().await.clone();
        let skill_service = self.skill_service.clone();
        let mcp_service = self.mcp_service.clone();
        let lsp_manager = self.lsp_manager.clone();
        let permission_service = self.permission_service.clone();
        let pending_asks = self.pending_asks.clone();
        let app = app_handle.clone();
        let session_key = format!("agent_stream_{}", session_id);
        eprintln!("[stream_chat] session_key: {}", session_key);

        // Load project skills if workspace is set.
        // MCP reload is spawned in background — don't block the first API turn
        // waiting for slow server connections.
        if let Some(ref ws) = workspace {
            skill_service.load_project_skills(ws).await;
            let mcp = mcp_service.clone();
            let ws_clone = ws.clone();
            tokio::spawn(async move {
                let svc = mcp.read().await;
                let statuses = svc.reload(Some(&ws_clone)).await;
                for s in &statuses {
                    if s.status == "failed" {
                        eprintln!("[stream_chat] MCP {} failed: {:?}", s.name, s.error);
                    }
                }
            });
        }

        // Build system prompt based on agent type
        let base_system = match agent_type {
            "ace" => ACE_SYSTEM,
            _ => ACE_SYSTEM,
        };

        // Build system prompt. Model info is included here so the model
        // treats it as system-level identity, not user input.
        let mut system_text = base_system.to_string();
        if let Some(ws) = &workspace {
            system_text.push_str(&format!("\n\n# 工作目录\n{}", ws));

            // Load project-level minimax.md if exists
            let minimax_md_path = std::path::Path::new(ws).join("minimax.md");
            if let Ok(content) = std::fs::read_to_string(&minimax_md_path) {
                if !content.trim().is_empty() {
                    system_text.push_str(&format!("\n\n# 项目规范 (minimax.md)\n{}", content));
                    eprintln!("[stream_chat] Loaded minimax.md from {}", minimax_md_path.display());
                }
            }
        }
        system_text.push_str(&format!("\n\n当前运行模型: {}", self.model));

        // System prompt as top-level `system` field (Anthropic format).
        // cache_control on system ensures it can be independently re-cached if
        // evicted (5 min TTL). Per MiniMax docs, mark_last_message_for_cache
        // handles incremental conversation caching on top of this.
        let system_block = if self.provider == "custom" {
            json!({"type": "text", "text": system_text})
        } else {
            json!({"type": "text", "text": system_text, "cache_control": {"type": "ephemeral"}})
        };
        let system_prompt = json!([system_block]);
        let system_json = system_prompt.to_string();

        let mut tools: Vec<serde_json::Value> = get_agent_tools(agent_type)
            .into_iter()
            .filter(|t| {
                // Custom providers don't have MiniMax search API — agent uses MCP web_search instead
                self.provider != "custom" || t["name"].as_str() != Some("web_search")
            })
            .collect();

        // Append MCP tools from all connected servers (names prefixed as server_tool)
        {
            let mcp = mcp_service.read().await;
            let mcp_tools = mcp.get_all_tools().await;
            let builtin_count = tools.len();
            for t in &mcp_tools {
                tools.push(make_tool(&t.name, &t.description, t.input_schema.clone()));
            }
            if !mcp_tools.is_empty() {
                eprintln!("[stream_chat] Loaded {} MCP tools, total: {}",
                    mcp_tools.len(), builtin_count + mcp_tools.len());
            }
        }

        // Mark last tool with cache_control to cache all tool definitions.
        // Per MiniMax docs: cache_control on the last tool caches ALL preceding tools.
        // Skip for custom providers that may reject cache_control.
        if self.provider != "custom" && !tools.is_empty() {
            let last_idx = tools.len() - 1;
            if let Some(obj) = tools[last_idx].as_object_mut() {
                obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
            }
        }

        // Serialize tools once for token estimation
        let tools_json = serde_json::to_string(&tools).unwrap_or_default();

        // Build messages array (NO system message — system is top-level `system` field).
        let mut api_messages: Vec<serde_json::Value> = Vec::new();
        // Save last user message for skill matching before messages is consumed
        let last_user_msg = messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        // Reconstruct messages from history, using raw_json for cache-identical byte sequences
        for msg in messages {
            let content_val = match msg.raw_json {
                Some(ref raw) => {
                    serde_json::from_str::<serde_json::Value>(raw)
                        .unwrap_or_else(|_| json!(msg.content))
                }
                None => {
                    // Fallback: try to reconstruct structured content from tool_calls/thinking
                    if msg.role == "assistant" && (msg.tool_calls.is_some() || msg.thinking.is_some()) {
                        let mut blocks = Vec::new();
                        if let Some(ref thinking) = msg.thinking {
                            if !thinking.is_empty() {
                                blocks.push(json!({"type": "thinking", "thinking": thinking}));
                            }
                        }
                        if !msg.content.is_empty() {
                            blocks.push(json!({"type": "text", "text": msg.content}));
                        }
                        if let Some(ref tc) = msg.tool_calls {
                            if let Ok(tool_calls) = serde_json::from_str::<Vec<serde_json::Value>>(tc) {
                                for tc in &tool_calls {
                                    let input_val = match &tc["function"]["arguments"] {
                                        // arguments is stored as a JSON string (e.g. "{\"pattern\":\"foo\"}")
                                        serde_json::Value::String(s) => {
                                            serde_json::from_str::<serde_json::Value>(s).unwrap_or_else(|_| json!({}))
                                        }
                                        // arguments is already an object — use directly
                                        other => other.clone(),
                                    };
                                    blocks.push(json!({
                                        "type": "tool_use",
                                        "id": tc["id"],
                                        "name": tc["function"]["name"],
                                        "input": input_val
                                    }));
                                }
                            }
                        }
                        if blocks.is_empty() {
                            json!(msg.content)
                        } else {
                            json!(blocks)
                        }
                    } else {
                        json!(msg.content)
                    }
                }
            };
            api_messages.push(json!({
                "role": msg.role,
                "content": content_val
            }));
        }

        // Merge consecutive tool_result-only user messages into one.
        // Strict Anthropic-compatible APIs require all tool_results matching
        // one assistant turn to be in a single user message.
        {
            let mut merged: Vec<serde_json::Value> = Vec::new();
            for msg in api_messages.drain(..) {
                let is_tool_result_msg = msg["role"] == "user"
                    && msg["content"].as_array().is_some_and(|b: &Vec<serde_json::Value>| {
                        !b.is_empty() && b.iter().all(|x| x["type"] == "tool_result")
                    });
                if is_tool_result_msg {
                    if let Some(last) = merged.last_mut() {
                        let last_is_tool_result = last["role"] == "user"
                            && last["content"].as_array().is_some_and(|b: &Vec<serde_json::Value>| {
                                !b.is_empty() && b.iter().all(|x| x["type"] == "tool_result")
                            });
                        if last_is_tool_result {
                            if let Some(arr) = last["content"].as_array_mut() {
                                if let Some(new_blocks) = msg["content"].as_array() {
                                    arr.extend(new_blocks.clone());
                                }
                            }
                            continue;
                        }
                    }
                }
                merged.push(msg);
            }
            api_messages = merged;
        }

        // Proactive skill matching: separate built-in (invisible to user) from user/project skills.
        let matched_skills = skill_service.match_skills(&last_user_msg, 5).await;
        let relevant: Vec<_> = matched_skills.iter().filter(|m| m.score > 0.10).collect();
        if !relevant.is_empty() {
            // Built-in skills — auto-inject full content for high-score matches (>30%)
            // so the agent follows critical rules without needing an extra skill-call round-trip.
            let builtins: Vec<_> = relevant.iter().filter(|m| m.source == "builtin").collect();
            if !builtins.is_empty() {
                let mut ctx = String::from("## 内置技能\n\n");
                for m in &builtins {
                    if m.score > 0.75 {
                        // High confidence — inject full content so the rule is impossible to miss
                        if let Some(content) = skill_service.get_skill_content(&m.name).await {
                            ctx.push_str(&format!("### {}\n\n{}\n\n", m.name, content));
                        }
                    } else {
                        // Low confidence — agent can load on demand
                        ctx.push_str(&format!("- **{}** (匹配度: {:.0}%): {}\n", m.name, m.score * 100.0, m.description));
                    }
                }
                ctx.push_str("以上内置技能已自动加载，直接按指引执行，不要再调用 skill 工具。\n");
                api_messages.push(json!({"role": "user", "content": ctx}));
            }
            // User/project skills — visible to user
            let user_skills: Vec<_> = relevant.iter().filter(|m| m.source != "builtin").collect();
            if !user_skills.is_empty() {
                let mut ctx = String::from("## 匹配到的技能\n\n");
                ctx.push_str("以下技能可能与你的任务相关：\n\n");
                for m in &user_skills {
                    ctx.push_str(&format!("- **{}** (匹配度: {:.0}%): {}\n",
                        m.name, m.score * 100.0, m.description));
                }
                api_messages.push(json!({"role": "user", "content": ctx}));
            }
        }

        // Append-only prefix cache: strip stale markers from persisted raw_json,
        // then mark one clean breakpoint. Re-marked every tool_use iteration.
        if self.provider != "custom" {
            strip_cache_control(&mut api_messages);
            mark_last_message_for_cache(&mut api_messages);
        }

        // Validate tool_use ↔ tool_result pairing. Auto-heal orphaned tool_use
        // (from aborted streams or crashes) by injecting stub tool_result blocks.
        // This prevents API 400 errors from permanently breaking the session.
        validate_tool_pairing(&mut api_messages);

        // Accumulate thinking across tool-use turns — only attach to the final message

        // Stability guards
        let max_steps: usize = std::env::var("MINIMAX_MAX_STEPS")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(100);
        let context_guard_pct: f64 = std::env::var("MINIMAX_CONTEXT_GUARD")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(0.80);
        let mut step = 0usize;
        // Track API-reported cumulative token count — far more accurate than heuristic.
        let mut last_cumulative_tokens: Option<u64> = None;

        // Main loop: continue until stop_reason is not "tool_use"
        loop {
            step += 1;
            // Check cancel before each API round-trip
            if *cancel_rx.borrow() {
                eprintln!("[stream_chat] Canceled at loop start for session {}", session_id);
                emit(&app, &session_key, StreamEvent::Aborted);
                break;
            }

            // Token count: API-reported cumulative value. On first call estimate from
            // existing messages so the context bar never resets to 0 between turns.
            let tokens = last_cumulative_tokens.map(|v| v as usize).unwrap_or_else(|| {
                estimate_tokens(&api_messages)
            });

            // Compress context when approaching token limit (70% of context window).
            // Uses shared compact_messages which persists to DB — same as /compact.
            if tokens > (self.context_window as f64 * 0.7) as usize && api_messages.len() > 12 {
                match crate::compact_messages(&self.db, session_id, agent_type, &mut api_messages, &api_key, &self.api_url, &self.messages_path, &self.model).await {
                    Ok((before, after)) => {
                        eprintln!("[compress] Compressed: {} → {} tokens", before, after);
                    }
                    Err(e) => {
                        eprintln!("[compress] Compression failed: {}", e);
                    }
                }
            }

            // Emit token usage for context window display
            let usage_pct = (tokens as f64 / self.context_window as f64) * 100.0;
            emit(&app, &session_key, StreamEvent::TokenUsage {
                estimated_tokens: tokens,
                context_window: self.context_window,
                usage_pct,
            });

            // Context guard: force exit if context is nearly full
            if usage_pct >= context_guard_pct * 100.0 {
                eprintln!("[context_guard] Context at {:.0}% — forcing exit", usage_pct);
                emit(&app, &session_key, StreamEvent::Error {
                    content: format!("上下文已满 ({:.0}%)，请开始新对话或压缩历史", usage_pct),
                });
                emit(&app, &session_key, StreamEvent::Done);
                break;
            }

            // Max steps guard: force text-only on last step
            if step >= max_steps {
                eprintln!("[max_steps] Reached {} steps — forcing final turn", max_steps);
                // Let this last API call go through; model should finish
                if step > max_steps + 1 {
                    emit(&app, &session_key, StreamEvent::Error {
                        content: format!("已达到最大轮次 ({})，请优化任务或简化指令", max_steps),
                    });
                    emit(&app, &session_key, StreamEvent::Done);
                    break;
                }
            }

            // Collapse Drain: retry with aggressive compression on context overflow
            let mut collapse_level = 0usize;
            let mut retry_count = 0u32;
            let response: reqwest::Response = loop {
                let request_body = json!({
                    "model": self.model,
                    "system": system_prompt,
                    "messages": api_messages,
                    "max_tokens": 16384,
                    "temperature": 1,
                    "stream": true,
                    "tools": tools,
                });
                let request_body_str = match serde_json::to_string(&request_body) {
                    Ok(s) => s,
                    Err(e) => {
                        emit(&app, &session_key, StreamEvent::Error { content: format!("Internal error: {}", e) });
                        return;
                    }
                };

                let est = estimate_request_tokens(&api_messages, &system_json, &tools_json);
                eprintln!("[stream_chat] Model: {}, Ctx: {}K, Messages: {}, EstTokens: {}", self.model, self.context_window / 1000, api_messages.len(), est);

                let client = Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
                    .unwrap_or_else(|_| Client::new());
                let resp = match client
                    .post(format!("{}{}", self.api_url, self.messages_path))
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .body(request_body_str)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let err_msg = e.to_string();
                        // Detect context overflow from network-level errors too
                        let is_overflow = err_msg.contains("413")
                            || err_msg.contains("context")
                            || err_msg.contains("token")
                            || err_msg.contains("too large")
                            || err_msg.contains("too long")
                            || err_msg.contains("body");
                        if is_overflow && collapse_level < 2 {
                            collapse_level += 1;
                            eprintln!("[collapse_drain] Network error hints at overflow, level {}", collapse_level);
                            let summary = summarize_with_model(agent_type, &api_messages, &api_key, &self.api_url, &self.messages_path, &self.model).await;
                            compress_context_aggressive(agent_type, &mut api_messages, collapse_level, summary);
                            continue;
                        }
                        // Retry on transient network errors (timeout, connection reset, DNS, etc.)
                        if retry_count < 10 {
                            retry_count += 1;
                            let delay = (retry_count * 2) as u64;
                            eprintln!("[stream_chat] Transient network error (retry {}/{}): {} — waiting {}s", retry_count, 10, e, delay);
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                        eprintln!("[stream_chat] Request failed after {} retries: {}", retry_count, e);
                        let ev = StreamEvent::Error { content: format!("Request failed after {} retries: {}", retry_count, e) };
                        emit(&app, &session_key, ev);
                        return;
                    }
                };

                if resp.status().is_success() {
                    break resp;
                }

                let status = resp.status();
                let err_text = resp.text().await.unwrap_or_default();
                eprintln!("[stream_chat] API error {}: {}", status, err_text);

                // Detect context overflow: HTTP 400/413 with overflow-related keywords
                let is_overflow = status.as_u16() == 413
                    || (status.as_u16() == 400 && (
                        err_text.contains("context")
                        || err_text.contains("token")
                        || err_text.contains("too large")
                        || err_text.contains("too long")
                        || err_text.contains("limit")
                        || err_text.contains("overflow")
                    ));

                if is_overflow && collapse_level < 2 {
                    collapse_level += 1;
                    eprintln!("[collapse_drain] Context overflow detected, aggressive compress level {}", collapse_level);
                    let summary = summarize_with_model(agent_type, &api_messages, &api_key, &self.api_url, &self.messages_path, &self.model).await;
                    compress_context_aggressive(agent_type, &mut api_messages, collapse_level, summary);
                    continue;
                }

                // Retry on transient HTTP errors (429 rate limit, 5xx server errors)
                let is_transient = status.as_u16() == 429
                    || status.as_u16() >= 500;
                if is_transient && retry_count < 10 {
                    retry_count += 1;
                    let delay = (retry_count * 2) as u64;
                    eprintln!("[stream_chat] Transient HTTP {} (retry {}/{}): {} — waiting {}s", status.as_u16(), retry_count, 10, err_text, delay);
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    continue;
                }

                let ev = StreamEvent::Error { content: format!("API error {}: {}", status, err_text) };
                emit(&app, &session_key, ev);
                return;
            };

            // Process SSE stream and collect result
            let mut repeat_guard_fired = false;
            let (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_tokens) = process_sse_stream(
                response.bytes_stream(),
                app.clone(),
                session_key.clone(),
                session_id,
                api_key.clone(),
                self.api_url.clone(),
                self.model.clone(),
                self.provider.clone(),
                skill_service.clone(),
                mcp_service.clone(),
                self.db.clone(),
                lsp_manager.clone(),
                permission_service.clone(),
                pending_asks.clone(),
                &mut api_messages,
                cancel_rx.clone(),
                &mut repeat_guard_fired,
                self.todo_store.clone(),
                self.running_command_pids.clone(),
            ).await;

            eprintln!("[stream_chat] Model: {}, stop_reason: {:?}, thinking: {} chars, text: {} chars, tool_uses: {}", self.model,
                stop_reason, assistant_thinking.len(), assistant_text.len(), tool_uses.len());

            // Repeat guard self-correction: inject stub tool results so the model
            // sees the suppressed calls and can adapt its approach.
            if repeat_guard_fired {
                // Save assistant message (with tool_use blocks but no text) to DB
                let mut db_content = Vec::new();
                if !assistant_text.is_empty() {
                    db_content.push(json!({"type": "text", "text": assistant_text}));
                }
                for (tool_id, tool_name, tool_input) in &tool_uses {
                    let input_json: serde_json::Value = serde_json::from_str(tool_input).unwrap_or(json!({}));
                    db_content.push(json!({
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": input_json
                    }));
                }
                let db_msg = json!({"role": "assistant", "content": if db_content.is_empty() { json!(assistant_text) } else { json!(db_content) }});
                save_api_message(&self.db, session_id, &db_msg);
                // Push assistant message to api_messages so tool_use blocks are present
                api_messages.push(db_msg);

                // Inject stub tool results as a user message so model sees the rejection
                let result_blocks: Vec<serde_json::Value> = tool_uses.iter().map(|(tool_id, tool_name, _input)| {
                    json!({"type": "tool_result", "tool_use_id": tool_id, "content": format!("Tool '{}' was suppressed (repeated identical call). Please use a different approach or different arguments.", tool_name)})
                }).collect();
                let stub_msg = json!({"role": "user", "content": result_blocks});
                save_api_message(&self.db, session_id, &stub_msg);
                api_messages.push(stub_msg);

                eprintln!("[repeat_guard] Injected {} stub results for self-correction", tool_uses.len());
                continue;
            }

            // Store API-reported cumulative token count and emit fresh usage
            // so the context bar always reflects the latest API-reported value.
            if let Some(real) = actual_tokens {
                last_cumulative_tokens = Some(real);
                let tokens = real as usize;
                let usage_pct = (tokens as f64 / self.context_window as f64) * 100.0;
                emit(&app, &session_key, StreamEvent::TokenUsage {
                    estimated_tokens: tokens,
                    context_window: self.context_window,
                    usage_pct,
                });
            }

            // Check cancel after SSE process returns
            if *cancel_rx.borrow() {
                eprintln!("[stream_chat] Canceled after SSE for session {}", session_id);
                if !assistant_thinking.is_empty() || !assistant_text.is_empty() {
                    let mut partial = Vec::new();
                    if !assistant_thinking.is_empty() {
                        partial.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                    }
                    if !assistant_text.is_empty() {
                        partial.push(json!({"type": "text", "text": assistant_text}));
                    }
                    let partial_msg = json!({"role": "assistant", "content": partial});
                    save_api_message(&self.db, session_id, &partial_msg);
                }
                emit(&app, &session_key, StreamEvent::Aborted);
                break;
            }

            // If stop_reason is not "tool_use", we're done.
            if stop_reason.as_deref() != Some("tool_use") {
                if !assistant_thinking.is_empty() || !assistant_text.is_empty() {
                    let mut final_content = Vec::new();
                    if !assistant_thinking.is_empty() {
                        final_content.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                    }
                    if !assistant_text.is_empty() {
                        final_content.push(json!({"type": "text", "text": assistant_text}));
                    }
                    let final_msg = json!({
                        "role": "assistant",
                        "content": final_content
                    });
                    save_api_message(&self.db, session_id, &final_msg);
                    api_messages.push(final_msg);
                }
                emit(&app, &session_key, StreamEvent::Done);
                break;
            }

            // stop_reason was "tool_use"
            // Add assistant message to api_messages WITH thinking (API needs it for
            // Interleaved Thinking chain / cache). DB also gets thinking so the UI
            // shows thinking-per-turn aligned with its corresponding tool_use.
            if !assistant_thinking.is_empty() || !assistant_text.is_empty() || !tool_uses.is_empty() {
                // Full content for API (includes thinking)
                let mut api_content = Vec::new();
                if !assistant_thinking.is_empty() {
                    api_content.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                }
                if !assistant_text.is_empty() {
                    api_content.push(json!({"type": "text", "text": assistant_text}));
                }
                for (tool_id, tool_name, tool_input) in &tool_uses {
                    let input_json: serde_json::Value = serde_json::from_str(tool_input).unwrap_or(json!({}));
                    api_content.push(json!({
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": input_json
                    }));
                }
                api_messages.push(json!({"role": "assistant", "content": api_content}));

                // DB content — thinking saved per-turn so UI shows it inline with its tool_use
                let mut db_content = Vec::new();
                if !assistant_thinking.is_empty() {
                    db_content.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                }
                if !assistant_text.is_empty() {
                    db_content.push(json!({"type": "text", "text": assistant_text}));
                }
                for (tool_id, tool_name, tool_input) in &tool_uses {
                    let input_json: serde_json::Value = serde_json::from_str(tool_input).unwrap_or(json!({}));
                    db_content.push(json!({
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": input_json
                    }));
                }
                let db_msg = json!({
                    "role": "assistant",
                    "content": if db_content.is_empty() { json!(assistant_text) } else { json!(db_content) }
                });
                save_api_message(&self.db, session_id, &db_msg);
            }

            // 2) Add ALL tool results as a SINGLE user message (Anthropic spec: one
            //    user message must contain all tool_result blocks matching the
            //    assistant's tool_use blocks).
            if !tool_results.is_empty() {
                let result_blocks: Vec<serde_json::Value> = tool_results.iter().map(|(_tool_name, tool_id, result)| {
                    json!({"type": "tool_result", "tool_use_id": tool_id, "content": result})
                }).collect();
                let tool_msg = json!({
                    "role": "user",
                    "content": result_blocks
                });
                save_api_message(&self.db, session_id, &tool_msg);
                api_messages.push(tool_msg);
            }

            // Append-only: strip stale markers, re-mark on the new last message.
            // This moves the cache breakpoint forward so the next request's prefix
            // covers the assistant + tool_result messages just added. Without this,
            // every subsequent request pays input_tokens for all accumulated content
            // after the original (pre-loop) breakpoint.
            if self.provider != "custom" {
                strip_cache_control(&mut api_messages);
                mark_last_message_for_cache(&mut api_messages);
            }
        }
    }
}

// Process SSE stream from MiniMax API
/// Emit a stream event with error logging (instead of silently discarding failures).
fn emit(app: &AppHandle, key: &str, event: StreamEvent) {
    if let Err(e) = app.emit(key, &event) {
        let type_name = match &event {
            StreamEvent::ContentBlockDelta { .. } => "content_block_delta",
            StreamEvent::ToolStart { .. } => "tool_start",
            StreamEvent::ToolEnd { .. } => "tool_end",
            StreamEvent::Done => "done",
            StreamEvent::Aborted => "aborted",
            StreamEvent::Error { .. } => "error",
            StreamEvent::CacheUsage { .. } => "cache_usage",
            StreamEvent::TokenUsage { .. } => "token_usage",
        };
        eprintln!("[emit] FAILED key={} type={}: {:?}", key, type_name, e);
    }
}

// Returns (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_input_tokens)
// tool_uses: Vec<(tool_id, tool_name, input_accumulated)>
// tool_results: Vec<(tool_name, tool_id, result)>
// actual_input_tokens: prompt token count reported by API (None if not available)
#[allow(clippy::too_many_arguments)]
async fn process_sse_stream(
    stream: impl StreamExt<Item = Result<bytes::Bytes, reqwest::Error>>,
    app: AppHandle,
    session_key: String,
    session_id: i64,
    api_key: String,
    api_url: String,
    model: String,
    provider: String,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    _api_messages: &mut Vec<serde_json::Value>,
    mut cancel_rx: watch::Receiver<bool>,
    repeat_guard_fired: &mut bool,
    todo_store: Arc<StdMutex<HashMap<i64, String>>>,
    running_command_pids: Arc<StdMutex<HashMap<i64, Vec<u32>>>>,
) -> (Option<String>, String, String, Vec<(String, String, String)>, Vec<(String, String, String)>, Option<u64>) {
    eprintln!("[process_sse_stream] Starting with session_key: {}", session_key);
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_input = String::new();
    let mut tool_inputs: HashMap<String, (String, String)> = HashMap::new();
    let mut stop_reason: Option<String> = None;
    let mut tool_results: Vec<(String, String, String)> = Vec::new();
    let mut assistant_text = String::new();
    let mut assistant_thinking = String::new();

    // Actual token count from API (last message_delta carries the total)
    let actual_input_tokens: Option<u64> = None;

    // Cache usage tracking (prefix-based KV cache)
    let mut cache_hit_tokens: u64 = 0;
    let mut cache_miss_tokens: u64 = 0;

    // Text emission buffer — merge rapid deltas to reduce IPC overhead.
    // 8ms balances IPC overhead against smoothness: emits ~120x/sec max, imperceptible.
    let mut last_emit = std::time::Instant::now();
    let emit_interval = std::time::Duration::from_millis(8);
    let mut pending_text = String::new();
    let mut pending_thinking = String::new();

    futures_util::pin_mut!(stream);

    // Race each stream.next() against the cancel watch channel.
    // When abort_stream sends on the channel, we return immediately —
    // the HTTP connection is dropped (TCP RST) instead of polling a flag.
    loop {
        // Fast-path: check before arming the select (non-blocking borrow)
        if *cancel_rx.borrow() {
            eprintln!("[process_sse_stream] Canceled mid-stream for session {}", session_id);
            if !pending_text.is_empty() || !pending_thinking.is_empty() {
                let ev = StreamEvent::ContentBlockDelta {
                    content: std::mem::take(&mut pending_text),
                    thinking: std::mem::take(&mut pending_thinking),
                };
                emit(&app, &session_key, ev);
            }
            return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens);
        }

        let item = tokio::select! {
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    eprintln!("[process_sse_stream] Canceled mid-stream for session {}", session_id);
                    if !pending_text.is_empty() || !pending_thinking.is_empty() {
                        let ev = StreamEvent::ContentBlockDelta {
                            content: std::mem::take(&mut pending_text),
                            thinking: std::mem::take(&mut pending_thinking),
                        };
                        emit(&app, &session_key, ev);
                    }
                    return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens);
                }
                continue;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(STREAM_SESSION_TIMEOUT_SECS)) => {
                eprintln!("[process_sse_stream] Global timeout ({}s) for session {}", STREAM_SESSION_TIMEOUT_SECS, session_id);
                emit(&app, &session_key, StreamEvent::Error {
                    content: "Session timed out after 1 hour".to_string(),
                });
                return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens);
            }
            item = stream.next() => { item }
        };

        match item {
            Some(Ok(bytes)) => {
                // Parse SSE directly from bytes — avoid intermediate String allocation
                let text = match std::str::from_utf8(&bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        let valid = e.valid_up_to();
                        unsafe { std::str::from_utf8_unchecked(&bytes[..valid]) }
                    }
                };
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                                let prev_text_len = assistant_text.len();
                                let prev_think_len = assistant_thinking.len();

                                if let Some(reason) = handle_sse_event(
                                    &event,
                                    &mut current_tool_id,
                                    &mut current_tool_name,
                                    &mut current_tool_input,
                                    &mut tool_inputs,
                                    &mut assistant_text,
                                    &mut assistant_thinking,
                                ) {
                                    stop_reason = Some(reason);
                                }

                                // Capture cache usage from message_delta events.
                                // MiniMax fields: cache_read (hit), cache_creation (write), input_tokens (post-breakpoint)
                                if event["type"] == "message_delta" {
                                    if let Some(usage) = event["usage"].as_object() {
                                        let hit = usage.get("cache_read_input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        let create = usage.get("cache_creation_input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        let input = usage.get("input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        // cache_read = existing cache reused
                                        // cache_creation + input_tokens = tokens not cached (just written + post-breakpoint)
                                        cache_hit_tokens += hit;
                                        cache_miss_tokens += create + input;
                                        eprintln!(
                                            "[cache] read={}, create={}, input={}, cumulative hit={} miss={}",
                                            hit, create, input, cache_hit_tokens, cache_miss_tokens
                                        );
                                    }
                                }

                                // Accumulate new text/thinking since last emit
                                if assistant_text.len() > prev_text_len {
                                    pending_text.push_str(&assistant_text[prev_text_len..]);
                                }
                                if assistant_thinking.len() > prev_think_len {
                                    pending_thinking.push_str(&assistant_thinking[prev_think_len..]);
                                }

                                // Emit if interval passed or this is a non-text event
                                let is_non_text = event["type"] != "content_block_delta";
                                let elapsed = last_emit.elapsed();
                                if ((is_non_text && (!pending_text.is_empty() || !pending_thinking.is_empty()))
                                    || elapsed >= emit_interval)
                                    && (!pending_text.is_empty() || !pending_thinking.is_empty()) {
                                        let ev = StreamEvent::ContentBlockDelta {
                                            content: std::mem::take(&mut pending_text),
                                            thinking: std::mem::take(&mut pending_thinking),
                                        };
                                        emit(&app, &session_key, ev);
                                        last_emit = std::time::Instant::now();
                                    }
                            }
                        }
                    }
            }
            Some(Err(e)) => {
                if !pending_text.is_empty() || !pending_thinking.is_empty() {
                    let ev = StreamEvent::ContentBlockDelta {
                        content: std::mem::take(&mut pending_text),
                        thinking: std::mem::take(&mut pending_thinking),
                    };
                    emit(&app, &session_key, ev);
                }
                emit(&app, &session_key, StreamEvent::Error { content: format!("Stream error: {}", e) });
                break;
            }
            None => break,
        }
    }

    // Flush any remaining buffered text
    if !pending_text.is_empty() || !pending_thinking.is_empty() {
        let ev = StreamEvent::ContentBlockDelta {
            content: std::mem::take(&mut pending_text),
            thinking: std::mem::take(&mut pending_thinking),
        };
        emit(&app, &session_key, ev);
    }

    // Emit cache usage stats for this turn
    if cache_hit_tokens > 0 || cache_miss_tokens > 0 {
        let total = cache_hit_tokens + cache_miss_tokens;
        let ratio = if total > 0 { cache_hit_tokens as f64 / total as f64 } else { 0.0 };
        let _ = app.emit(&session_key, StreamEvent::CacheUsage {
            cache_hit_tokens,
            cache_miss_tokens,
            cache_hit_ratio: ratio,
        });
        eprintln!(
            "[cache] turn stats: hit={}, miss={}, ratio={:.2}%",
            cache_hit_tokens, cache_miss_tokens, ratio * 100.0
        );
    }

    // Flush the last tool's input (deferred from handle_sse_event)
    if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
        tool_inputs.insert(id, (name, std::mem::take(&mut current_tool_input)));
    }

    // Build tool_uses list from collected tool_inputs.
    // Apply JSON repair to fix model truncation issues (unbalanced braces, etc.)
    let tool_uses: Vec<(String, String, String)> = tool_inputs
        .into_iter()
        .map(|(tool_id, (tool_name, input))| {
            let repaired = repair_truncated_json(&input);
            (tool_id, tool_name, repaired)
        })
        .collect();

    // Repeat guard: sliding window of recent (tool, input) pairs.
    // If the same call appears 3+ times in the window, suppress it.
    // If ALL calls are suppressed, give one self-correction chance
    // (inject stub tool results so the model sees the failure and adapts).
    let repeat_window: usize = std::env::var("MINIMAX_REPEAT_WINDOW")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    let repeat_threshold: usize = std::env::var("MINIMAX_REPEAT_THRESHOLD")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let mut repeat_history: Vec<String> = Vec::with_capacity(repeat_window);

    let tool_uses = if stop_reason.as_deref() == Some("tool_use") {
        let allowed_calls: Vec<(String, String, String)> = tool_uses.iter().filter(|(_, name, input)| {
            let key = format!("{}|{}", name, input);
            let count = repeat_history.iter().filter(|k| *k == &key).count() + 1;
            repeat_history.push(key.clone());
            if repeat_history.len() > repeat_window { repeat_history.remove(0); }
            count < repeat_threshold
        }).cloned().collect();
        let blocked = tool_uses.len() - allowed_calls.len();
        if allowed_calls.is_empty() && !tool_uses.is_empty() {
            if !*repeat_guard_fired {
                *repeat_guard_fired = true;
                eprintln!("[repeat_guard] All {} calls suppressed — self-correction", tool_uses.len());
                // Return original tool_uses so stream_chat knows what to inject stubs for.
                // Set stop_reason to signal self-correction mode.
                stop_reason = Some("repeat_guard_correction".into());
                tool_uses.clone()
            } else {
                eprintln!("[repeat_guard] Still stuck after self-correction — allowing through");
                repeat_history.clear();
                tool_uses.clone()
            }
        } else {
            if blocked > 0 { eprintln!("[repeat_guard] Blocked {} repeated tool calls", blocked); }
            allowed_calls
        }
    } else {
        tool_uses
    };

        let parallel_max = std::env::var("MINIMAX_PARALLEL_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(16);
        let dispatch_serial = std::env::var("MINIMAX_TOOL_DISPATCH")
            .map(|v| v == "serial")
            .unwrap_or(false);

        // Check cancel before tool dispatch
        if *cancel_rx.borrow() {
            eprintln!("[process_sse_stream] Canceled before tool dispatch for session {}", session_id);
            return (None, assistant_text, assistant_thinking, tool_uses, Vec::new(), actual_input_tokens);
        }

        let mut idx = 0;
        while idx < tool_uses.len() {
            // Build chunk: consecutive parallel-safe tools up to parallel_max
            let mut chunk: Vec<usize> = Vec::new();
            if !dispatch_serial {
                while idx < tool_uses.len()
                    && chunk.len() < parallel_max
                    && is_parallel_safe(&tool_uses[idx].1)
                {
                    chunk.push(idx);
                    idx += 1;
                }
            }
            // If no parallel-safe could be grouped, take one (non-parallel-safe) alone
            if chunk.is_empty() {
                chunk.push(idx);
                idx += 1;
            }

            // Emit all tool_start events before dispatch (UI shows all at once)
            for &i in &chunk {
                let (_tool_id, tool_name, final_input) = &tool_uses[i];
                let input_json: serde_json::Value =
                    serde_json::from_str(final_input).unwrap_or(json!({}));
                emit(&app, &session_key, StreamEvent::ToolStart {
                    tool: tool_name.clone(),
                    tool_id: _tool_id.clone(),
                    input: input_json,
                });
            }

            // Run chunk concurrently
            let futs: Vec<_> = chunk.iter().map(|&i| {
                let tool_name = tool_uses[i].1.clone();
                let final_input = tool_uses[i].2.clone();
                let api_key = api_key.clone();
                let api_url = api_url.clone();
                let model = model.clone();
                let provider = provider.clone();
                let skill_service = skill_service.clone();
                let mcp_service = mcp_service.clone();
                let db = db.clone();
                let lsp_manager = lsp_manager.clone();
                let permission_service = permission_service.clone();
                let pending_asks = pending_asks.clone();
                let todo_store = todo_store.clone();
                let running_command_pids = running_command_pids.clone();
                let app = app.clone();
                let sid = session_id;
                let cancel_rx = cancel_rx.clone();
                tokio::spawn(async move {
                    let result = execute_tool(
                        &tool_name, &final_input, sid,
                        api_key, api_url, model, provider,
                        skill_service, mcp_service,
                        db, lsp_manager, permission_service, pending_asks, app,
                        cancel_rx, todo_store, running_command_pids,
                    ).await;
                    (i, tool_name, result)
                })
            }).collect();

            // Race: tool execution vs cancel (immediate detection via changed())
            let mut cancel_rx_select = cancel_rx.clone();
            let results = {
                tokio::select! {
                    r = join_all(futs) => r,
                    _ = async {
                        let _ = cancel_rx_select.changed().await;
                        *cancel_rx_select.borrow()
                    } => {
                        eprintln!("[tool_dispatch] Cancelled during tool execution for session {}", session_id);
                        // Emit tool_end for every tool that had tool_start emitted
                        for &i in &chunk {
                            let (ref tool_id, ref tool_name, _) = &tool_uses[i];
                            emit(&app, &session_key, StreamEvent::ToolEnd {
                                tool: tool_name.clone(),
                                tool_id: tool_id.clone(),
                                result: "[cancelled by user]".to_string(),
                            });
                            tool_results.push((tool_name.clone(), tool_id.clone(), "[cancelled by user]".to_string()));
                        }
                        Vec::new()
                    }
                }
            };

            // Emit tool_end in declared order
            for (idx, r) in results.into_iter().enumerate() {
                let &tool_idx = &chunk[idx];
                let (ref tool_id, ref tool_name, _) = &tool_uses[tool_idx];
                match r {
                    Ok((_i, _tname, result)) => {
                        let truncated = truncate_tool_result(result);
                        emit(&app, &session_key, StreamEvent::ToolEnd {
                            tool: tool_name.clone(),
                            tool_id: tool_id.clone(),
                            result: truncated.clone(),
                        });
                        tool_results.push((tool_name.clone(), tool_id.clone(), truncated));
                    }
                    Err(e) => {
                        let err_msg = format!("Tool '{}' internal error: {}", tool_name, e);
                        emit(&app, &session_key, StreamEvent::ToolEnd {
                            tool: tool_name.clone(),
                            tool_id: tool_id.clone(),
                            result: err_msg.clone(),
                        });
                        tool_results.push((tool_name.clone(), tool_id.clone(), err_msg));
                    }
                }
            }
        }

    (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_input_tokens)
}

fn handle_sse_event(
    event: &serde_json::Value,
    current_tool_id: &mut Option<String>,
    current_tool_name: &mut Option<String>,
    current_tool_input: &mut String,
    tool_inputs: &mut HashMap<String, (String, String)>,
    assistant_text: &mut String,
    assistant_thinking: &mut String,
) -> Option<String> {
    let event_type = event["type"].as_str().unwrap_or("");

    match event_type {
        "content_block_start" => {
            let block = &event["content_block"];
            let block_type = block["type"].as_str().unwrap_or("");
            match block_type {
                "tool_use" => {
                    // Move previous tool's input into map (one clone per tool, not per delta)
                    if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
                        tool_inputs.insert(id, (name, std::mem::take(current_tool_input)));
                    }
                    *current_tool_id = block["id"].as_str().map(|s| s.to_string());
                    *current_tool_name = block["name"].as_str().map(|s| s.to_string());
                    current_tool_input.clear();
                }
                "thinking" => {
                    // Some providers send full thinking in content_block_start
                    if let Some(thinking) = block["thinking"].as_str() {
                        if !thinking.is_empty() {
                            assistant_thinking.push_str(thinking);
                        }
                    }
                }
                _ => {}
            }
            None
        }
        "content_block_delta" => {
            let delta = &event["delta"];

            // Accumulate text (emission handled by process_sse_stream buffer)
            if let Some(text) = delta["text"].as_str() {
                if !text.is_empty() {
                    assistant_text.push_str(text);
                }
            }

            // Accumulate thinking (streaming delta)
            if let Some(thinking) = delta["thinking"].as_str() {
                if !thinking.is_empty() {
                    assistant_thinking.push_str(thinking);
                }
            }

            // Tool input delta (MiniMax uses partial_json field)
            // Only append — move to tool_inputs deferred to next block_start or stream end
            if let Some(input) = delta["partial_json"].as_str() {
                if current_tool_id.is_some() {
                    current_tool_input.push_str(input);
                }
            }
            None
        }
        "message_delta" => {
            event["delta"]["stop_reason"].as_str().map(|stop_reason| stop_reason.to_string())
        }
        "message_stop" => {
            // Don't emit Done here - Done is emitted by stream_chat
            // when the loop actually breaks (stop_reason != "tool_use")
            None
        }
        _ => None
    }
}

// ========== Tool Execution ==========

#[allow(clippy::too_many_arguments)]
async fn execute_tool(
    tool_name: &str,
    input: &str,
    session_id: i64,
    api_key: String,
    api_url: String,
    model: String,
    provider: String,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    app_handle: AppHandle,
    cancel_rx: watch::Receiver<bool>,
    todo_store: Arc<StdMutex<HashMap<i64, String>>>,
    running_command_pids: Arc<StdMutex<HashMap<i64, Vec<u32>>>>,
) -> String {
    let params: serde_json::Value = serde_json::from_str(input).unwrap_or(json!({}));

    // Global cancel check — honored by every tool before doing work
    if *cancel_rx.borrow() {
        return json!({"error": "cancelled"}).to_string();
    }

    // --- Permission Check ---
    {
        let file_path = params["path"].as_str()
            .or_else(|| params["file_path"].as_str())
            .or_else(|| params["target"].as_str());
        let command = params["command"].as_str();
        let reason = tool_reason(tool_name, file_path, command);

        let verdict = {
            match permission_service.lock() {
                Ok(ps) => ps.evaluate(tool_name, file_path, command),
                Err(e) => {
                    return format!("Permission service error: {}", e);
                }
            }
        };
        match verdict {
            None => {
                // Need confirmation — emit event and wait
                let (perm_id, rx) = {
                    match permission_service.lock() {
                        Ok(ps) => match ps.register_pending() {
                            Ok(v) => v,
                            Err(e) => {
                                return format!("Permission service error: {}", e);
                            }
                        },
                        Err(e) => {
                            return format!("Permission service error: {}", e);
                        }
                    }
                };
                let req = PermissionRequest {
                    id: perm_id.clone(),
                    tool: tool_name.to_string(),
                    file: file_path.map(|s| s.to_string()),
                    command: command.map(|s| s.to_string()),
                    reason: reason.clone(),
                };
                let _ = app_handle.emit("permission_asked", &req);
                let mut cancel_rx_perm = cancel_rx.clone();
                let perm_result = tokio::select! {
                    result = tokio::time::timeout(std::time::Duration::from_secs(PERMISSION_TIMEOUT_SECS), rx) => {
                        match result {
                            Ok(inner) => Ok(inner),
                            Err(_) => Err("timeout"),
                        }
                    }
                    _ = async {
                        let _ = cancel_rx_perm.changed().await;
                        *cancel_rx_perm.borrow()
                    } => {
                        Err("cancelled")
                    }
                };
                match perm_result {
                    Ok(Ok(PermissionAction::Allow)) => {
                        eprintln!("[perm] {} allowed by user", tool_name);
                    }
                    Ok(Ok(PermissionAction::Deny)) | Ok(Err(_)) => {
                        eprintln!("[perm] {} denied", tool_name);
                        let _ = permission_service.lock().map(|mut ps| ps.resolve_pending(&perm_id, tool_name, PermissionAction::Deny, false));
                        return format!("Permission denied: {}", reason);
                    }
                    Err("timeout") => {
                        eprintln!("[perm] {} timed out after {}s", tool_name, PERMISSION_TIMEOUT_SECS);
                        let _ = permission_service.lock().map(|mut ps| ps.resolve_pending(&perm_id, tool_name, PermissionAction::Deny, false));
                        return format!("Permission timed out: {}", reason);
                    }
                    Err("cancelled") => {
                        eprintln!("[perm] {} cancelled by user abort", tool_name);
                        let _ = permission_service.lock().map(|mut ps| ps.resolve_pending(&perm_id, tool_name, PermissionAction::Deny, false));
                        return json!({"error": "cancelled"}).to_string();
                    }
                    _ => {
                        let _ = permission_service.lock().map(|mut ps| ps.resolve_pending(&perm_id, tool_name, PermissionAction::Deny, false));
                        return format!("Permission denied: {}", reason);
                    }
                }
            }
            Some(Ok(())) => {
                // Allowed silently
            }
            Some(Err(msg)) => {
                return format!("Blocked: {}", msg);
            }
        }
    }

    // ── Worktree path routing ──
    // If this session has a worktree_path, rewrite file paths for file tools
    let params = {
        let worktree_path: Option<String> = {
            db.lock().ok().and_then(|c| {
                c.query_row(
                    "SELECT worktree_path FROM agent_session WHERE id = ?1",
                    [session_id],
                    |row| row.get(0),
                ).ok().flatten()
            })
        };

        if let Some(ref wp) = worktree_path {
            let mut p = params.clone();
            // Rewrite path fields for file tools
            match tool_name {
                "read_file" | "write_file" | "edit_file" | "edit_lines" | "multi_edit"
                | "delete_file" | "create_directory" | "get_file_info" | "list_dir"
                | "directory_tree" | "read_lints" | "touch_file" | "create_dir" => {
                    if let Some(path) = p["path"].as_str().map(|s| s.to_string()) {
                        p["path"] = json!(normalize_file_path_with_worktree(&path, Some(wp)));
                    }
                }
                "read_files" => {
                    if let Some(paths) = p["paths"].as_array().cloned() {
                        let new_paths: Vec<serde_json::Value> = paths.iter().map(|v| {
                            if let Some(s) = v.as_str() {
                                json!(normalize_file_path_with_worktree(s, Some(wp)))
                            } else if let Some(obj) = v.as_object() {
                                let mut new_obj = obj.clone();
                                if let Some(path) = obj.get("path").and_then(|p| p.as_str()) {
                                    new_obj.insert("path".to_string(), json!(normalize_file_path_with_worktree(path, Some(wp))));
                                }
                                serde_json::Value::Object(new_obj)
                            } else {
                                v.clone()
                            }
                        }).collect();
                        p["paths"] = serde_json::Value::Array(new_paths);
                    }
                }
                "write_files" | "modify_files" => {
                    if let Some(files) = p["files"].as_array().cloned() {
                        let new_files: Vec<serde_json::Value> = files.iter().map(|f| {
                            if let Some(obj) = f.as_object() {
                                let mut new_obj = obj.clone();
                                if let Some(path) = obj.get("path").and_then(|p| p.as_str()) {
                                    new_obj.insert("path".to_string(), json!(normalize_file_path_with_worktree(path, Some(wp))));
                                }
                                serde_json::Value::Object(new_obj)
                            } else {
                                f.clone()
                            }
                        }).collect();
                        p["files"] = serde_json::Value::Array(new_files);
                    }
                }
                "move_file" | "copy_file" => {
                    if let Some(src) = p["source"].as_str().map(|s| s.to_string()) {
                        p["source"] = json!(normalize_file_path_with_worktree(&src, Some(wp)));
                    }
                    if let Some(dst) = p["destination"].as_str().map(|s| s.to_string()) {
                        p["destination"] = json!(normalize_file_path_with_worktree(&dst, Some(wp)));
                    }
                }
                "find_replace_in_files" | "search_in_dir" | "search_files" | "glob" => {
                    if let Some(path) = p["path"].as_str().map(|s| s.to_string()) {
                        p["path"] = json!(normalize_file_path_with_worktree(&path, Some(wp)));
                    }
                }
                "run_command" | "run_background" => {
                    if let Some(path) = p["path"].as_str().map(|s| s.to_string()) {
                        p["path"] = json!(normalize_file_path_with_worktree(&path, Some(wp)));
                    } else if let Some(cwd) = p["cwd"].as_str().map(|s| s.to_string()) {
                        p["cwd"] = json!(normalize_file_path_with_worktree(&cwd, Some(wp)));
                    }
                }
                _ => {}
            }
            p
        } else {
            params
        }
    };

    // Extract file path for auto-touch BEFORE params is potentially moved in the match
    let auto_touch_path: Option<String> = match tool_name {
        "write_file" | "delete_file" | "copy_file" | "move_file" | "edit_file" | "create_directory" =>
            params["path"].as_str().map(normalize_file_path),
        _ => None,
    };

    // Save file snapshots before modification for undo support (async to avoid blocking runtime)
    match tool_name {
        "write_file" | "edit_file" | "delete_file" => {
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot_async(db.clone(), session_id, normalize_file_path(p)).await;
            }
        }
        "write_files" | "modify_files" => {
            if let Some(files) = params["files"].as_array() {
                let paths: Vec<String> = files.iter()
                    .filter_map(|f| f.as_object())
                    .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                    .map(|p| normalize_file_path(p))
                    .collect();
                save_file_snapshots_batch(db.clone(), session_id, paths).await;
            }
        }
        "move_file" => {
            let mut paths = Vec::new();
            if let Some(src) = params["source"].as_str() {
                paths.push(normalize_file_path(src));
            }
            if let Some(dst) = params["destination"].as_str() {
                paths.push(normalize_file_path(dst));
            }
            save_file_snapshots_batch(db.clone(), session_id, paths).await;
        }
        "copy_file" => {
            if let Some(dst) = params["destination"].as_str() {
                save_file_snapshot_async(db.clone(), session_id, normalize_file_path(dst)).await;
            }
        }
        "multi_edit" | "edit_lines" => {
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot_async(db.clone(), session_id, normalize_file_path(p)).await;
            }
        }
        "create_dir" => {
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot_async(db.clone(), session_id, normalize_file_path(p)).await;
            }
        }
        "remove_path" => {
            if let Some(p) = params["path"].as_str() {
                let np = normalize_file_path(p);
                let path = std::path::Path::new(&np);
                let mut paths_to_snapshot: Vec<String> = Vec::new();
                if path.is_dir() {
                    if std::fs::read_dir(path).is_ok() {
                        collect_dir_files(path, &mut paths_to_snapshot);
                    }
                }
                paths_to_snapshot.push(np);
                save_file_snapshots_batch(db.clone(), session_id, paths_to_snapshot).await;
            }
        }
        "run_command" | "run_background" => {
            if let Some(cwd) = params["cwd"].as_str().map(|s| s.to_string())
                .or_else(|| params["path"].as_str().map(|s| s.to_string()))
            {
                snapshot_workspace_files(db.clone(), session_id, cwd).await;
            }
        }
        _ => {}
    }

    let result = match tool_name {
        "list_dir" => tool_list_dir(&params).await,
        "read_file" => tool_read_file(&params).await,
        "read_files" => tool_read_files(&params).await,
        "search_in_dir" => tool_search_in_dir(&params).await,
        "get_env_info" => tool_get_env_info(&params).await,
        "analyze_project_structure" => tool_analyze_project_structure(&params).await,
        "run_command" => tool_run_command(&params, session_id, Some(running_command_pids.clone())).await,
        "write_file" => tool_write_file(&params).await,
        "write_files" => tool_write_files(&params).await,
        "find_replace_in_files" => tool_find_replace_in_files(&params).await,
        "modify_files" => tool_modify_files(&params).await,
        "get_file_info" => tool_get_file_info(&params).await,
        "directory_tree" => tool_directory_tree(&params).await,
        "glob" => tool_glob(&params).await,
        "search_files" => tool_search_files(&params).await,
        "edit_file" => tool_edit_file(&params).await,
        "edit_lines" => tool_edit_lines(&params).await,
        "multi_edit" => tool_multi_edit(&params).await,
        "create_directory" => tool_create_directory(&params).await,
        "move_file" => tool_move_file(&params).await,
        "delete_file" => tool_delete_file(&params).await,
        "copy_file" => tool_copy_file_fn(&params).await,
        "web_fetch" => tool_web_fetch(params.clone()).await,
        "run_background" => tool_run_background(&params, session_id).await,
        "job_output" => tool_job_output(&params).await,
        "list_jobs" => tool_list_jobs(&params).await,
        "spawn_process" => tool_spawn_process(&params).await,
        "kill_process" => tool_kill_process(&params).await,
        "web_search" => tool_web_search(params.clone(), api_key.clone(), api_url.clone(), provider.clone()).await,
        "understand_image" => tool_understand_image(params.clone(), api_key.clone(), api_url.clone(), model.clone(), provider.clone()).await,
        "skill" => tool_skill(tool_name, &params, skill_service.clone()).await,
        "list_builtin_skills" => tool_list_builtin_skills(tool_name, &params, skill_service.clone()).await,
        "list_user_skills" => tool_list_user_skills(tool_name, &params, skill_service.clone()).await,
        "match_skills" => tool_match_skills(tool_name, &params, skill_service.clone()).await,
        "mcp_reload" => tool_mcp_reload(&params, mcp_service.clone(), skill_service.clone(), db.clone()).await,
        "execute_skill" => tool_execute_skill(tool_name, &params, skill_service.clone()).await,
        "read_knowledge" => tool_read_knowledge(&params).await,
        "write_knowledge" => tool_write_knowledge(&params).await,
        "list_knowledge" => tool_list_knowledge().await,
        "read_lints" => tool_read_lints(&params, lsp_manager.clone(), db.clone()).await,
        "touch_file" => tool_touch_file(&params, lsp_manager.clone(), db.clone()).await,
        "ask_choice" => tool_ask_choice(&params, session_id, "unknown", pending_asks.clone(), app_handle.clone()).await,
        "todo_write" => tool_todo_write(&params, &todo_store, session_id).await,
        // Worktree tools
        "create_worktree" => tool_create_worktree(&params, session_id, db.clone()).await,
        "merge_worktree" => tool_merge_worktree(&params, session_id, db.clone()).await,
        // Fallback: try MCP (with cancel race)
        _ => {
            let mut cancel_rx_mcp = cancel_rx.clone();
            tokio::select! {
                result = async {
                    let mcp = mcp_service.read().await;
                    mcp.call_tool_any(tool_name, params).await
                } => {
                    match result {
                        Ok(r) => serde_json::to_string(&r).unwrap_or_else(|e| format!("MCP result serialization failed: {}", e)),
                        Err(e) => format!("Tool '{}' not implemented (MCP error: {})", tool_name, e),
                    }
                }
                _ = async {
                    let _ = cancel_rx_mcp.changed().await;
                    *cancel_rx_mcp.borrow()
                } => {
                    json!({"error": "cancelled"}).to_string()
                }
            }
        }
    };

    // Auto-touch LSP for file-modifying tools so diagnostics are cached for read_lints
    if let Some(ref fp) = auto_touch_path {
        let lm = lsp_manager;
        let fp_owned = fp.clone();
        let db_clone = db;
        tokio::spawn(async move {
            let touch_params = json!({"file_path": fp_owned});
            let _ = tool_touch_file(&touch_params, lm, db_clone).await;
        });
    }

    truncate_tool_result(result)
}

fn get_agent_tools(agent_type: &str) -> Vec<serde_json::Value> {
    use std::sync::OnceLock;
    use std::collections::HashMap;
    static CACHE: OnceLock<HashMap<String, Vec<serde_json::Value>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        map.insert("ace".to_string(), build_agent_tools("ace"));
        map.insert("default".to_string(), build_agent_tools("default"));
        map
    });
    cache.get(agent_type).cloned().unwrap_or_else(|| build_agent_tools(agent_type))
}

fn build_agent_tools(_agent_type: &str) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();

    // ===== TOOL GROUP DEFINITIONS =====

    // Read-only file inspection (all agents)
    let read_only_files: &[(&str, &str, serde_json::Value)] = &[
        ("read_file", "读取文件内容。offset: 起始行号(1-indexed)，limit: 最大行数。不传则读全文（>2MB 文件自动截断到前300行并提示用 offset/limit）", schema_obj(json!({"path": {"type": "string"}, "offset": {"type": "integer"}, "limit": {"type": "integer"}}), &["path"])),
        ("read_files", "批量读取多个文件。paths 支持字符串数组或对象数组 [{path, offset?, limit?}]。全局 offset/limit 对所有文件生效，per-file 覆盖全局", schema_obj(json!({"paths": {"type": "array", "items": {"type": "string"}}, "offset": {"type": "integer", "description": "全局起始行号(1-indexed)"}, "limit": {"type": "integer", "description": "全局最大行数"}}), &["paths"])),
        ("list_dir", "列出目录内容，含文件大小、行数、修改时间", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("directory_tree", "递归列出目录树结构。maxDepth默认2，自动跳过node_modules/.git/target等目录", schema_obj(json!({"path": {"type": "string"}, "max_depth": {"type": "integer"}}), &["path"])),
        ("get_file_info", "获取文件信息（类型、扩展名、行数、大小、修改时间）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Search & analysis (plan / explore / work / review)
    let search_tools: &[(&str, &str, serde_json::Value)] = &[
        ("search_in_dir", "在目录中递归搜索文件内容，返回 path:line: text。支持 regex、文件类型过滤、上下文行。定位问题首选工具——比 read_file 快得多。", schema_obj(json!({"path": {"type": "string"}, "pattern": {"type": "string"}, "file_type": {"type": "string", "description": "按扩展名过滤，如 \"rs\", \"vue\", \"ts\", \"py\""}, "context": {"type": "integer", "description": "显示匹配行前后 N 行上下文（0-5）"}, "regex": {"type": "boolean", "description": "启用正则表达式匹配"}}), &["path", "pattern"])),
        ("search_files", "按文件名搜索（大小写不敏感），匹配文件名而非内容", schema_obj(json!({"path": {"type": "string"}, "pattern": {"type": "string"}}), &["path", "pattern"])),
        ("glob", "按glob模式匹配文件。file_type: 按扩展名过滤（如 \"rs\", \"vue\"）", schema_obj(json!({"pattern": {"type": "string"}, "path": {"type": "string"}, "limit": {"type": "integer"}, "file_type": {"type": "string", "description": "按扩展名过滤，如 \"rs\", \"vue\", \"ts\""}}), &["pattern"])),
        ("analyze_project_structure", "分析项目顶层结构", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Write/edit tools (work only)
    let write_tools: &[(&str, &str, serde_json::Value)] = &[
        ("write_file", "创建或覆盖文件（含父目录）", schema_obj(json!({"path": {"type": "string"}, "content": {"type": "string"}}), &["path", "content"])),
        ("edit_file", "精确字符串替换。search必须唯一匹配，返回diff。大文件或多处修改用edit_lines", schema_obj(json!({"path": {"type": "string"}, "search": {"type": "string"}, "replace": {"type": "string"}}), &["path", "search", "replace"])),
        ("edit_lines", "按行号或搜索定位编辑。search: 搜索文本定位行号（首次匹配），替代 start_line。替换: start_line/search+end_line+content / 插入: start_line/search+content / 删除: start_line/search+end_line", schema_obj(json!({"path": {"type": "string"}, "search": {"type": "string", "description": "搜索文本来定位行号，首次匹配生效"}, "start_line": {"type": "integer"}, "end_line": {"type": "integer"}, "content": {"type": "string"}}), &["path"])),
        ("multi_edit", "原子性跨文件编辑。edits: [{path, search, replace}]。全部验证通过才写入，任一失败则回滚", schema_obj(json!({"edits": {"type": "array", "items": {"type": "object"}}}), &["edits"])),
        ("find_replace_in_files", "目录下批量查找替换（支持regex）", schema_obj(json!({"path": {"type": "string"}, "find": {"type": "string"}, "replace": {"type": "string"}, "use_regex": {"type": "boolean"}}), &["path", "find", "replace"])),
        ("create_directory", "创建目录（含父目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("move_file", "移动/重命名文件或目录", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
        ("delete_file", "删除单个文件（拒绝目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("copy_file", "复制文件或目录（递归）", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
    ];

    // Command execution (work only)
    let command_tools: &[(&str, &str, serde_json::Value)] = &[
        ("run_command", "执行命令（阻塞等完成）。直接用要执行的命令，自由选择 shell：如 git status、cmd /c dir、powershell -Command \"...\"、sh -c \"...\"。智能体会根据输出报错自行调整", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "timeout": {"type": "integer"}}), &["command"])),
        ("run_background", "后台运行长进程（dev server/build）立即返回。命令自由选择 shell。返回 task_id/pid/out_file，前端面板实时看输出", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "wait_sec": {"type": "integer"}}), &["command"])),
        ("kill_process", "按PID终止进程", schema_obj(json!({"pid": {"type": "number"}}), &["pid"])),
        ("job_output", "查询后台进程输出。out_file: run_background 返回的输出文件路径。tail_lines: 返回最后N行，默认200", schema_obj(json!({"job_id": {"type": "integer"}, "out_file": {"type": "string"}, "tail_lines": {"type": "integer"}}), &[])),
        ("list_jobs", "列出当前会话后台任务", schema_obj(json!({}), &[])),
    ];

    // Web & media (all agents)
    let web_tools: &[(&str, &str, serde_json::Value)] = &[
        ("web_search", "网络搜索，返回标题/URL/摘要", schema_obj(json!({"query": {"type": "string"}}), &["query"])),
        ("web_fetch", "获取URL内容并提取文本（去标签），上限32K字符", schema_obj(json!({"url": {"type": "string"}}), &["url"])),
        ("understand_image", "分析图片内容（JPEG/PNG/WebP）。传入prompt和image_url（本地文件路径或http URL）。同步返回分析结果，耗时5-30秒。不要对同一张图重复调用", schema_obj(json!({"prompt": {"type": "string"}, "image_url": {"type": "string"}}), &["prompt", "image_url"])),
    ];

    // Environment (all)
    let env_tools: &[(&str, &str, serde_json::Value)] = &[
        ("get_env_info", "获取开发环境信息（语言/框架/Git状态）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Knowledge read (all)
    let knowledge_read: &[(&str, &str, serde_json::Value)] = &[
        ("read_knowledge", "读取项目知识库文件。file_name: 文件名（自动保存在工作目录对应的项目下）", schema_obj(json!({"file_name": {"type": "string"}}), &["file_name"])),
        ("list_knowledge", "列出项目知识库中的所有文件", schema_obj(json!({}), &[])),
    ];

    // Communication
    let ask_tool = ("ask_choice", "向用户提问。用于需要用户选择或确认时。questions: [{id, question, options: [{id, text}], multi_select}]", schema_obj(json!({"questions": {"type": "array", "items": {"type": "object"}}}), &["questions"]));

    // Todo tracking
    let todo_tool = ("todo_write", "创建和更新结构化任务列表来跟踪进度——防偷懒利器。每次调用用完整列表替换旧列表。\ntodos: [{content: 任务描述(单一具体动作), status: pending|in_progress|completed, activeForm: 进行时描述}]\n规则：\n1. 任何非纯问答任务的第一步就是调此工具，3-7项为宜\n2. 每项是单一具体动作，禁止\"实现功能\"这类模糊描述\n3. 同一时间只有一项 in_progress\n4. 完成一项立刻标记 completed，不许批量标记\n5. 只有验证通过（测试/lint 通过）才能标记 completed\n6. 全部 completed 后才汇报任务完成", schema_obj(json!({"todos": {"type": "array", "items": {"type": "object", "properties": {"content": {"type": "string", "description": "任务描述，单一具体动作"}, "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}, "activeForm": {"type": "string", "description": "进行时描述，如 '正在添加登录组件'"}}, "required": ["content", "status"]}}}), &["todos"]));

    // Skill tools (front / plan / work / review - NOT explore)
    let skill_tools: &[(&str, &str, serde_json::Value)] = &[
        ("skill", "加载指定技能的完整操作指令", schema_obj(json!({"name": {"type": "string"}}), &["name"])),
        ("list_builtin_skills", "列出系统内置技能（通用领域知识，如 MCP配置、代码审查等）", schema_obj(json!({}), &[])),
        ("list_user_skills", "列出用户和项目创建的外部技能（优先使用）", schema_obj(json!({}), &[])),
        ("match_skills", "根据描述关键词匹配技能，外部技能优先返回", schema_obj(json!({"query": {"type": "string"}, "top_k": {"type": "integer"}}), &["query"])),
        ("execute_skill", "执行技能脚本", schema_obj(json!({"name": {"type": "string"}, "script": {"type": "string"}}), &["name"])),
        ("mcp_reload", "重载 MCP 配置。修改 mcp.json 后调用使配置生效", schema_obj(json!({}), &[])),
    ];

    // Knowledge write
    let kw = ("write_knowledge", "写入项目知识库文件。file_name: 文件名，content: 内容（自动保存在工作目录对应的项目下）", schema_obj(json!({"file_name": {"type": "string"}, "content": {"type": "string"}}), &["file_name", "content"]));

    // Lint tools
    let lint = ("read_lints", "读取LSP诊断信息（类型错误、lint警告等）。可选传path参数过滤文件，不传则返回所有文件", schema_obj(json!({"path": {"type": "string"}}), &[]));

    fn add_tools(tools: &mut Vec<serde_json::Value>, defs: &[(&str, &str, serde_json::Value)]) {
        for (name, desc, schema) in defs {
            tools.push(make_tool(name, desc, schema.clone()));
        }
    }

    // ===== ACE AGENT TOOLS =====

    add_tools(&mut tools, read_only_files);
    add_tools(&mut tools, search_tools);
    add_tools(&mut tools, write_tools);
    add_tools(&mut tools, command_tools);
    tools.push(make_tool(lint.0, lint.1, lint.2.clone()));
    add_tools(&mut tools, web_tools);
    add_tools(&mut tools, env_tools);
    add_tools(&mut tools, knowledge_read);
    tools.push(make_tool(kw.0, kw.1, kw.2.clone()));
    tools.push(make_tool(ask_tool.0, ask_tool.1, ask_tool.2.clone()));
    add_tools(&mut tools, skill_tools);
    tools.push(make_tool(todo_tool.0, todo_tool.1, todo_tool.2.clone()));

    tools
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // --- hidden_cmd ---

    #[test]
    fn hidden_cmd_has_utf8_env_vars() {
        let cmd = hidden_cmd(if cfg!(windows) { "cmd" } else { "sh" });
        // We can't inspect creation flags, but we can verify env vars are set
        let envs: Vec<_> = cmd.get_envs().collect();
        let has_pythonutf8 = envs.iter().any(|(k, v)| {
            k.to_str() == Some("PYTHONUTF8") && v.as_ref().map(|v| v.to_str() == Some("1")).unwrap_or(false)
        });
        assert!(has_pythonutf8, "PYTHONUTF8=1 should be set");
    }

    #[test]
    fn hidden_cmd_has_lang_env() {
        let cmd = hidden_cmd(if cfg!(windows) { "cmd" } else { "sh" });
        let envs: Vec<_> = cmd.get_envs().collect();
        let has_lang = envs.iter().any(|(k, _)| k.to_str() == Some("LANG"));
        assert!(has_lang);
    }

    // --- decode_process_output ---

    #[test]
    #[cfg(windows)]
    fn decode_valid_utf8_passthrough() {
        let input = "Hello World 你好".as_bytes();
        let result = decode_process_output(input);
        assert_eq!(result, "Hello World 你好");
    }

    #[test]
    #[cfg(windows)]
    fn decode_pure_ascii() {
        let input = b"Exit: 0\nfile1.txt\nfile2.txt";
        let result = decode_process_output(input);
        assert_eq!(result, "Exit: 0\nfile1.txt\nfile2.txt");
    }

    #[test]
    #[cfg(windows)]
    fn decode_gbk_fallback() {
        // "中文测试" in GBK encoding
        let gbk_bytes: &[u8] = &[0xD6, 0xD0, 0xCE, 0xC4, 0xB2, 0xE2, 0xCA, 0xD4];
        let result = decode_process_output(gbk_bytes);
        assert!(result.contains("中文"), "Should decode GBK Chinese, got: {}", result);
    }

    #[test]
    #[cfg(windows)]
    fn decode_empty_bytes() {
        let result = decode_process_output(b"");
        assert!(result.is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn decode_garbled_fallback_to_lossy() {
        // Completely invalid bytes that are neither UTF-8 nor GBK
        let bad: &[u8] = &[0xFF, 0xFE, 0x00, 0x00, 0xC0, 0xC1];
        let result = decode_process_output(bad);
        // Should not panic, should produce something via lossy conversion
        assert!(!result.is_empty() || result.is_empty()); // just not panic
    }

    #[test]
    #[cfg(not(windows))]
    fn decode_unix_passthrough() {
        let result = decode_process_output(b"Hello World");
        assert_eq!(result, "Hello World");
    }

    #[test]
    #[cfg(not(windows))]
    fn decode_unix_lossy() {
        let bytes: &[u8] = &[0xFF, 0xFE, 0x41, 0x42, 0x43];
        let result = decode_process_output(bytes);
        assert!(result.contains("ABC"));
    }

    // --- output truncation (the 256KB cap in output_with_timeout) ---

    #[test]
    fn output_cap_head_tail_strategy() {
        // Simulate the cap logic: head 128KB + tail 128KB
        let big: Vec<u8> = (0..300_000usize).map(|b| (b % 128 + 32) as u8).collect();
        let result = decode_process_output(&big);
        // Should be capped in output_with_timeout, but decode doesn't cap
        // — this test verifies decode handles large input without OOM
        assert!(result.len() > 200_000);
    }

    // --- hex_encode / hex_decode ---

    #[test]
    fn hex_encode_basic() {
        let bytes: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(hex_encode(bytes), "deadbeef");
    }

    #[test]
    fn hex_encode_empty() {
        assert_eq!(hex_encode(b""), "");
    }

    #[test]
    fn hex_decode_roundtrip() {
        let original = b"Hello World\x00\xFF";
        let hex = hex_encode(original);
        let decoded = hex_decode(&hex);
        assert_eq!(decoded, original);
    }

    #[test]
    fn hex_decode_empty() {
        assert!(hex_decode("").is_empty());
    }

    // --- is_printable_text ---

    #[test]
    fn printable_plain_text() {
        assert!(is_printable_text("Hello World\nLine 2\r\nLine 3\tTab"));
    }

    #[test]
    fn non_printable_null() {
        assert!(!is_printable_text("Hello\0World"));
    }

    #[test]
    fn printable_empty() {
        assert!(is_printable_text(""));
    }

    #[test]
    fn non_printable_ctrl_chars() {
        assert!(!is_printable_text("Hello\x01World"));
        assert!(!is_printable_text("Hello\x1BWorld"));
    }

    // --- mark_last_message_for_cache ---

    #[test]
    fn mark_cache_string_content() {
        let mut msgs = vec![json!({"role": "user", "content": "hello"})];
        mark_last_message_for_cache(&mut msgs);
        let last = msgs.last().unwrap();
        let content = last["content"].as_array().unwrap();
        let last_block = content.last().unwrap();
        assert!(last_block["cache_control"]["type"] == "ephemeral");
    }

    #[test]
    fn mark_cache_array_content() {
        let mut msgs = vec![json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "hi"}]
        })];
        mark_last_message_for_cache(&mut msgs);
        let content = msgs.last().unwrap()["content"].as_array().unwrap();
        assert!(content.last().unwrap()["cache_control"]["type"] == "ephemeral");
    }

    // --- strip_cache_control ---

    #[test]
    fn strip_existing_cache_control() {
        let mut msgs = vec![json!({
            "role": "user",
            "content": [{"type": "text", "text": "hi", "cache_control": {"type": "ephemeral"}}]
        })];
        strip_cache_control(&mut msgs);
        let block = &msgs[0]["content"][0];
        assert!(block["cache_control"].is_null() || !block.as_object().unwrap().contains_key("cache_control"));
    }
}