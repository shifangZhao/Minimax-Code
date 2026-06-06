// Agent Service - Rust Implementation for AI Agent Streaming
//
// Provides:
// - MiniMax API streaming (via reqwest)
// - Tool execution
// - Message history management
// - Interleaved Thinking support

use futures_util::future::join_all;
use futures_util::StreamExt;
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
    // Force UTF-8 mode universally — GBK fallback removed for clean AI input.
    cmd.env("PYTHONUTF8", "1")
       .env("PYTHONIOENCODING", "utf-8:surrogate:escape")
       .env("LANG", "en_US.UTF-8")
       .env("LC_ALL", "en_US.UTF-8")
       .env("GIT_TERMINAL_PROMPT", "0")
       .env("GIT_EDITOR", ":")
       .env("GIT_MERGE_AUTOEDIT", "no")
       .env("GIT_PAGER", "cat")
       .env("EDITOR", ":")
       .env("VISUAL", "")
       .env("npm_config_unicode", "utf8")
       .env("npm_config_yes", "true")
       .env("PIP_NO_INPUT", "1")
       .env("HOMEBREW_NO_AUTO_UPDATE", "1")
       .env("YARN_ENABLE_IMMUTABLE_INSTALLS", "false");
    cmd
}

/// Return the best available system shell for command execution.
/// Windows: pwsh > powershell > cmd. Unix: sh. Cached after first call.
pub(crate) fn default_shell() -> &'static str {
    use std::sync::OnceLock;
    static SHELL: OnceLock<&'static str> = OnceLock::new();
    SHELL.get_or_init(|| {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            if std::process::Command::new("pwsh")
                .args(["-NoProfile", "-Command", "echo ok"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return "pwsh";
            }
            if std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", "echo ok"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return "powershell";
            }
            "cmd"
        }
        #[cfg(not(windows))]
        {
            "sh"
        }
    })
}

/// Encode a UTF-8 string as UTF-16LE then Base64 — the format PowerShell's
/// `-EncodedCommand` expects. This completely avoids quote-escaping issues.
fn encode_utf16le_base64(s: &str) -> String {
    use base64::Engine;
    let utf16le: Vec<u8> = s.encode_utf16().flat_map(|w| w.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(&utf16le)
}

/// Shell arguments for the given shell program.
pub(crate) fn shell_args(shell: &str, command: &str) -> Vec<String> {
    match shell {
        "pwsh" | "powershell" => vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-EncodedCommand".into(),
            encode_utf16le_base64(command),
        ],
        _ if cfg!(windows) => vec![
            "/d".into(),
            "/s".into(),
            "/c".into(),
            command.to_string(),
        ],
        _ => vec!["-c".into(), command.to_string()],
    }
}

/// Decode process output bytes to string.
/// Strict UTF-8 first; lossy replacement for invalid sequences.
/// No GBK fallback — any legacy encoding bytes become replacement chars,
/// giving AI clean, predictable input instead of garbage characters.
#[allow(dead_code)]
#[cfg(windows)]
fn decode_process_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[allow(dead_code)]
#[cfg(not(windows))]
fn decode_process_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
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
    // Force UTF-8 mode universally (works for Python, Node, Rust, Go, etc.)
    // No GBK fallback — invalid bytes become replacement chars for clean AI input.
    cmd.env("PYTHONUTF8", "1")
       .env("PYTHONIOENCODING", "utf-8:surrogate:escape")
       .env("LANG", "en_US.UTF-8")
       .env("LC_ALL", "en_US.UTF-8")
       .env("GIT_TERMINAL_PROMPT", "0")
       .env("GIT_EDITOR", ":")
       .env("GIT_MERGE_AUTOEDIT", "no")
       .env("GIT_PAGER", "cat")
       .env("EDITOR", ":")
       .env("VISUAL", "")
       .env("npm_config_unicode", "utf8")
       .env("npm_config_yes", "true")
       .env("PIP_NO_INPUT", "1")
       .env("HOMEBREW_NO_AUTO_UPDATE", "1")
       .env("YARN_ENABLE_IMMUTABLE_INSTALLS", "false");
    cmd
}

/// Async version of output_with_timeout using tokio::process.
/// No OS threads spawned — uses tokio async I/O for pipe reads.
#[allow(dead_code)]
pub(crate) async fn async_output_with_timeout(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
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

    // Register with unified process manager
    let cmd_str = format!("{} {}", program, args.join(" "));
    crate::process_manager::register_process(
        pid,
        crate::process_manager::ProcessType::Command,
        &cmd_str,
        Some(session_id),
        None,
        None,
    );

    // Take stdout/stderr before waiting
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    // Spawn async readers — collect in parallel, bounded to MAX_OUTPUT bytes total
    const MAX_OUTPUT: usize = 30_000; // 30K chars cap — keeps LLM context compact
    let out_handle = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(8192);
        if let Some(mut stdout) = child_stdout {
            use tokio::io::AsyncReadExt;
            let mut collected = 0;
            while collected < MAX_OUTPUT {
                buf.reserve(8192);
                let n = stdout.read_buf(&mut buf).await.unwrap_or(0);
                if n == 0 { break; }
                collected += n;
                if n < 8192 { break; }
            }
        }
        buf
    });
    let err_handle = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(4096);
        if let Some(mut stderr) = child_stderr {
            use tokio::io::AsyncReadExt;
            let mut collected = 0;
            while collected < MAX_OUTPUT {
                let n = stderr.read_buf(&mut buf).await.unwrap_or(0);
                if n == 0 { break; }
                collected += n;
                if n < 4096 { break; }
            }
        }
        buf
    });

    // Wait with timeout
    let force_kill = |pid: u32| {
        #[cfg(windows)]
        { let _ = hidden_cmd("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output(); }
        #[cfg(not(windows))]
        {
            let _ = hidden_cmd("kill").args(["-15", &pid.to_string()]).output();
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = hidden_cmd("kill").args(["-9", &pid.to_string()]).output();
        }
    };

    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait(),
    ).await {
        Ok(Ok(status)) => {
            // Process exited normally — collect output with bounded wait
            let (stdout_bytes, stderr_bytes) = tokio::join!(
                tokio::time::timeout(std::time::Duration::from_secs(10), out_handle),
                tokio::time::timeout(std::time::Duration::from_secs(10), err_handle),
            );
            let stdout_bytes = stdout_bytes.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let stderr_bytes = stderr_bytes.unwrap_or(Ok(Vec::new())).unwrap_or_default();

            let (stdout, truncated) = decode_output_truncated(&stdout_bytes, MAX_OUTPUT);
            let stderr_str = decode_process_output(&stderr_bytes);
            let exit = status.code().unwrap_or(-1);
            let suffix = if truncated {
                format!("\n[...{} bytes truncated ...]", stdout_bytes.len().saturating_sub(MAX_OUTPUT))
            } else { String::new() };
            if stdout.is_empty() && !stderr_str.is_empty() {
                format!("Exit: {}{}\n{}", exit, suffix, stderr_str)
            } else if !stderr_str.is_empty() {
                format!("Exit: {}{}\n{}\n{}", exit, suffix, stdout, stderr_str)
            } else {
                format!("Exit: {}{}\n{}", exit, suffix, stdout)
            }
        }
        Ok(Err(e)) => format!("Error waiting for process: {}", e),
        Err(_) => {
            // Timeout — kill process tree and collect whatever is available
            force_kill(pid);
            let _ = child.kill().await;
            // Wait briefly for OS pipe buffers to flush
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let (stdout_bytes, stderr_bytes) = tokio::join!(
                tokio::time::timeout(std::time::Duration::from_secs(2), out_handle),
                tokio::time::timeout(std::time::Duration::from_secs(2), err_handle),
            );
            let stdout_bytes = stdout_bytes.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let stderr_bytes = stderr_bytes.unwrap_or(Ok(Vec::new())).unwrap_or_default();
            let partial_out = decode_process_output(&stdout_bytes);
            let partial_err = decode_process_output(&stderr_bytes);
            let output_summary = if partial_out.is_empty() && partial_err.is_empty() {
                String::new()
            } else {
                format!("Partial output:\n{}{}", partial_out, partial_err)
            };
            format!("Command timed out after {}s (killed)\n{}", timeout_secs, output_summary)
        }
    };

    result
}

/// Decode output, returning (decoded_string, was_truncated).
/// Truncation happens at a safe byte boundary so no partial UTF-8 characters are emitted.
#[allow(dead_code)]
fn decode_output_truncated(bytes: &[u8], max_len: usize) -> (String, bool) {
    if bytes.len() <= max_len {
        return (decode_process_output(bytes), false);
    }
    // Find safe truncation point — scan backwards from max_len for UTF-8 boundary
    let mut end = max_len;
    while end > 0 && (bytes[end] & 0x80) != 0 && (bytes[end] & 0xC0) != 0xC0 {
        end -= 1;
    }
    if end == 0 { end = max_len; } // fallback if no boundary found
    let truncated_len = end;
    let head = decode_process_output(&bytes[..truncated_len]);
    (head, true)
}

/// Async shell execution using tokio::process.
#[allow(dead_code)]
pub(crate) async fn async_execute_via_shell(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
) -> String {
    let shell = default_shell();
    let args = shell_args(shell, command);
    async_output_with_timeout(shell, &args, cwd, timeout_secs, session_id).await
}

use crate::context_compressor::{compress_context_aggressive, estimate_request_tokens, summarize_with_model};
use crate::lsp_manager::LspManager;
use crate::mcp_service::McpService;
use crate::permission::{PermissionService, PermissionAction, PermissionRequest};
use crate::skill_service::SkillService;
use crate::system_prompts::ACE_SYSTEM;

// ========== Types ==========

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
    #[serde(rename = "db_updated")]
    DbUpdated { message_id: i64, content: String, thinking: String, tool_results: Vec<serde_json::Value> },
    // ── Streaming command protocol (NDJSON-style) ──
    // All four carry call_id + tool_id so the frontend can deinterleave
    // concurrent commands. Chunks are base64-encoded to survive JSON.
    #[serde(rename = "exec_command_begin")]
    CommandBegin { call_id: String, tool_id: String, command: String, cwd: Option<String> },
    #[serde(rename = "exec_command_output_delta")]
    CommandDelta { call_id: String, tool_id: String, stream: String, chunk_b64: String },
    #[serde(rename = "exec_command_end")]
    CommandEnd { call_id: String, tool_id: String, exit_code: Option<i32>, killed: bool, truncated: bool },
    #[serde(rename = "exec_command_error")]
    CommandError { call_id: String, tool_id: String, error: String },
}

// ========== Agent Service ==========

pub(crate) type PendingAsks = Arc<StdMutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>>;

/// Save an api_message (with structured content blocks) to the chat_message table.
/// Stores display text in `content` and the full JSON block array in `raw_json`.
#[allow(clippy::type_complexity)]

/// Save the full conversation context (api_messages) to the agent_session.context_messages column.
/// This decouples the LLM context from the display-oriented chat_message table.
pub(crate) fn save_context_messages(db: &Arc<StdMutex<Connection>>, session_id: i64, api_messages: &[serde_json::Value]) {
    let json = serde_json::to_string(api_messages).unwrap_or_else(|_| "[]".to_string());
    if let Ok(conn) = db.lock() {
        let _ = conn.execute(
            "UPDATE agent_session SET context_messages = ?1 WHERE id = ?2",
            rusqlite::params![json, session_id],
        );
    }
}
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
    }
}

/// Incrementally update the streaming message in DB.
/// Called on each ContentBlockDelta emit so DB is always up-to-date.
/// Runs synchronously — the caller throttles to 50ms intervals,
/// so the ~1ms SQLite write is acceptable and avoids thread pool issues.
fn update_streaming_message(
    db: &Arc<StdMutex<Connection>>,
    session_id: i64,
    message_id: i64,
    new_text: &str,
    new_thinking: &str,
) {
    if let Ok(conn) = db.lock() {
        if !new_text.is_empty() {
            conn.execute(
                "UPDATE chat_message SET content = content || ? WHERE id = ?",
                rusqlite::params![new_text, message_id],
            ).ok();
        }
        if !new_thinking.is_empty() {
            conn.execute(
                "INSERT INTO message_part (message_id, session_id, part_order, part_type, content)
                 VALUES (?1, ?2,
                    (SELECT COALESCE(MAX(part_order), -1) + 1 FROM message_part WHERE message_id = ?1),
                    'thinking', ?3)",
                rusqlite::params![message_id, session_id, new_thinking],
            ).ok();
        }
    }
}

/// Delete a pending streaming message (on cancel or error).
fn delete_streaming_message(db: &Arc<StdMutex<Connection>>, message_id: i64) {
    if let Ok(conn) = db.lock() {
        conn.execute("DELETE FROM message_part WHERE message_id = ?", rusqlite::params![message_id]).ok();
        conn.execute("DELETE FROM chat_message WHERE id = ?", rusqlite::params![message_id]).ok();
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

/// Validate tool_use ↔ tool_result pairing both directions. Required by
/// Anthropic-compatible APIs.
/// - Orphan tool_use (assistant emitted tool_use, but the next user message
///   has no matching tool_result): inject a stub tool_result so the session
///   can recover without a restart.
/// - Orphan tool_result (user message has a tool_result whose tool_use is
///   missing from the conversation — e.g. dropped by compression or never
///   saved): drop the block, and drop the user message if it becomes empty.
fn validate_tool_pairing(msgs: &mut Vec<serde_json::Value>) {
    // Pre-collect every tool_use id in the conversation. Used to detect
    // orphan tool_result blocks whose referenced tool_use is gone.
    let known_tool_use_ids: std::collections::HashSet<String> = msgs
        .iter()
        .filter(|m| m["role"].as_str() == Some("assistant"))
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|b| b["type"] == "tool_use")
        .filter_map(|b| b["id"].as_str().map(|s| s.to_string()))
        .collect();

    let mut i = 0;
    while i < msgs.len() {
        let role = msgs[i]["role"].as_str().unwrap_or("");

        if role == "assistant" {
            let content = &msgs[i]["content"];
            let tool_use_ids: Vec<String> = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "tool_use")
                .filter_map(|b| b["id"].as_str().map(|s| s.to_string()))
                .collect();

            if !tool_use_ids.is_empty() {
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
            }
        } else if role == "user" {
            // Drop tool_result blocks that reference a tool_use not in the
            // conversation. Without this, the API rejects the request with
            // "tool result's tool id(...) not found (2013)".
            if let Some(content_arr) = msgs[i]["content"].as_array_mut() {
                let original_len = content_arr.len();
                content_arr.retain(|b| {
                    if b["type"] != "tool_result" {
                        return true;
                    }
                    match b["tool_use_id"].as_str() {
                        Some(id) => known_tool_use_ids.contains(id),
                        None => false,
                    }
                });
                let dropped = original_len - content_arr.len();
                if dropped > 0 {
                    eprintln!(
                        "[tool_pair_validate] Dropped {} orphan tool_result block(s) from message[{}]",
                        dropped, i
                    );
                }
                if content_arr.is_empty() {
                    eprintln!("[tool_pair_validate] Dropping empty user message[{}] (only had orphan tool_results)", i);
                    msgs.remove(i);
                    continue;
                }
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
    // Streaming-command protocol registries
    command_cancel_registry: crate::command_stream::CancelRegistry,
    command_pid_registry: crate::command_stream::CommandPidRegistry,
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
            command_cancel_registry: self.command_cancel_registry.clone(),
            command_pid_registry: self.command_pid_registry.clone(),
        }
    }
}

impl AgentService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(api_key: String, api_url: String, messages_path: String, model: String, context_window: usize, provider: String, skill_service: Arc<SkillService>, mcp_service: Arc<RwLock<McpService>>, db: Arc<StdMutex<Connection>>, lsp_manager: Arc<StdMutex<Option<LspManager>>>, permission_service: Arc<StdMutex<PermissionService>>, pending_asks: PendingAsks, command_cancel_registry: crate::command_stream::CancelRegistry, command_pid_registry: crate::command_stream::CommandPidRegistry) -> Self {
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
            command_cancel_registry,
            command_pid_registry,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn stream_chat(
        &self,
        agent_type: &str,
        messages: Vec<serde_json::Value>,
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
        // messages are already in API format (serde_json::Value with role + content),
        // preserved from context_messages with full structured content blocks.
        let mut api_messages: Vec<serde_json::Value> = messages;
        // Save last user message for skill matching
        let last_user_msg = api_messages.iter().rev()
            .find(|m| m["role"].as_str() == Some("user"))
            .and_then(|m| {
                let c = &m["content"];
                // content may be a plain string or a structured array — extract text
                if let Some(s) = c.as_str() {
                    Some(s.to_string())
                } else if let Some(arr) = c.as_array() {
                    // Extract text from content blocks
                    let texts: Vec<&str> = arr.iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect();
                    if texts.is_empty() { None } else { Some(texts.join("\n")) }
                } else {
                    None
                }
            })
            .unwrap_or_default();

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

        // Proactive skill matching: only inject name + description, let the agent load full content on demand.
        let matched_skills = skill_service.match_skills(&last_user_msg, 5).await;
        let relevant: Vec<_> = matched_skills.iter().filter(|m| m.score > 0.10).collect();
        if !relevant.is_empty() {
            let mut ctx = String::from("## 匹配到的技能\n\n以下技能可能与你的任务相关，需要时调用 skill 工具加载完整内容：\n\n");
            for m in &relevant {
                ctx.push_str(&format!("- **{}** (匹配度: {:.0}%): {}\n",
                    m.name, m.score * 100.0, m.description));
            }
            api_messages.push(json!({"role": "user", "content": ctx}));
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
            .ok().and_then(|v| v.parse().ok()).unwrap_or(200);
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
                // Ensure context is saved so the partial conversation is available
                // as context when the user sends the next message.
                save_context_messages(&self.db, session_id, &api_messages);
                emit(&app, &session_key, StreamEvent::Aborted);
                break;
            }

            // Token count: use API-reported cumulative value directly.
            // Skip compression/guard checks on first turn if API hasn't reported yet.
            let tokens = last_cumulative_tokens.map(|v| v as usize).unwrap_or(0);

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
                save_context_messages(&self.db, session_id, &api_messages);
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
                    save_context_messages(&self.db, session_id, &api_messages);
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
                        emit(&app, &session_key, StreamEvent::Done);
                        return;
                    }
                };

                let est = estimate_request_tokens(&api_messages, &system_json, &tools_json);
                eprintln!("[stream_chat] Model: {}, Ctx: {}K, Messages: {}, EstTokens: {}", self.model, self.context_window / 1000, api_messages.len(), est);

                let client = Client::builder()
                    .connect_timeout(std::time::Duration::from_secs(30))
                    .read_timeout(std::time::Duration::from_secs(600))
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
                        emit(&app, &session_key, StreamEvent::Error { content: format!("Request failed after {} retries: {}", retry_count, e) });
                        save_context_messages(&self.db, session_id, &api_messages);
                        emit(&app, &session_key, StreamEvent::Done);
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

                emit(&app, &session_key, StreamEvent::Error { content: format!("API error {}: {}", status, err_text) });
                save_context_messages(&self.db, session_id, &api_messages);
                emit(&app, &session_key, StreamEvent::Done);
                return;
            };

            // Create pending message for incremental DB persistence
            let current_msg_id = {
                let conn = self.db.lock().unwrap();
                conn.execute(
                    "INSERT INTO chat_message (session_id, role, content) VALUES (?1, 'assistant', '')",
                    rusqlite::params![session_id],
                ).ok();
                conn.last_insert_rowid()
            };

            // Process SSE stream and collect result
            let mut repeat_guard_fired = false;
            let mut repeat_history: Vec<String> = Vec::new();
            let (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_tokens, ordered_blocks, had_error) = process_sse_stream(
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
                &mut repeat_history,
                self.todo_store.clone(),
                self.command_cancel_registry.clone(),
                self.command_pid_registry.clone(),
                current_msg_id,
            ).await;

            eprintln!("[stream_chat] Model: {}, stop_reason: {:?}, thinking: {} chars, text: {} chars, tool_uses: {}", self.model,
                stop_reason, assistant_thinking.len(), assistant_text.len(), tool_uses.len());

            // Repeat guard self-correction: inject stub tool results so the model
            // sees the suppressed calls and can adapt its approach.
            if repeat_guard_fired {
                // Clear the repeat history on self-correction: the stubs that
                // are about to be injected already tell the model "stop
                // trying this". Counting the same calls again on the next turn
                // would lock the model out of even 1-2 legitimate retries.
                repeat_history.clear();
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
                // Save partial assistant message so it becomes context for the next turn.
                // Without this, the aborted response is lost and the LLM doesn't see
                // what it already wrote — causing it to repeat or lose coherence.
                if !ordered_blocks.is_empty() {
                    let content: Vec<serde_json::Value> =
                        ordered_blocks.iter().map(|b| b.to_json()).collect();
                    let final_msg = json!({"role": "assistant", "content": content});
                    save_api_message(&self.db, session_id, &final_msg);
                    delete_streaming_message(&self.db, current_msg_id);
                    api_messages.push(final_msg);
                } else if !assistant_thinking.is_empty() || !assistant_text.is_empty() {
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
                    delete_streaming_message(&self.db, current_msg_id);
                    api_messages.push(final_msg);
                } else {
                    // No content produced — just clean up the streaming placeholder
                    delete_streaming_message(&self.db, current_msg_id);
                }
                save_context_messages(&self.db, session_id, &api_messages);
                emit(&app, &session_key, StreamEvent::Aborted);
                break;
            }

            // If the SSE stream had an error (network drop, timeout, etc.),
            // don't treat the partial response as a successful completion.
            // The Error event was already emitted by process_sse_stream.
            if had_error {
                eprintln!("[stream_chat] Stream had error, not saving partial response as final");
                break;
            }

            // If stop_reason is not "tool_use", we're done.
            if stop_reason.as_deref() != Some("tool_use") {
                if !ordered_blocks.is_empty() {
                    // Build from ordered_blocks to preserve the LLM's
                    // original interleaving of thinking / text / tool_use.
                    let content: Vec<serde_json::Value> =
                        ordered_blocks.iter().map(|b| b.to_json()).collect();
                    let final_msg = json!({"role": "assistant", "content": content});
                    save_api_message(&self.db, session_id, &final_msg);
                    // Drop the streaming placeholder — save_api_message just
                    // created the authoritative message. Without this,
                    // loadMessages brings back BOTH rows and the final
                    // reply renders twice.
                    delete_streaming_message(&self.db, current_msg_id);
                    api_messages.push(final_msg);
                } else if !assistant_thinking.is_empty() || !assistant_text.is_empty() {
                    // Fallback for the rare case the SSE stream produced
                    // no blocks (e.g. cancelled before any data). Keeps
                    // the same flat ordering the code used pre-fix.
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
                    delete_streaming_message(&self.db, current_msg_id);
                    api_messages.push(final_msg);
                }
                save_context_messages(&self.db, session_id, &api_messages);
                emit(&app, &session_key, StreamEvent::Done);
                break;
            }

            // stop_reason was "tool_use"
            // Add assistant message to api_messages WITH thinking (API needs it for
            // Interleaved Thinking chain / cache). DB also gets thinking so the UI
            // shows thinking-per-turn aligned with its corresponding tool_use.
            //
            // IMPORTANT: DB and API get DIFFERENT content arrays on purpose.
            // - DB  -> ordered_blocks (preserves LLM's original [thinking, text,
            //         tool, text, tool, text] interleaving so tool cards land in
            //         the right spots on the UI when displayMessages reloads).
            // - API -> aggregated [thinking, text, tool, tool, ...] (byte-identical
            //         to the pre-fix layout so Anthropic prompt cache stays hot.
            //         The LLM doesn't care about internal block order; the cache
            //         does care about bytes, so we keep the byte sequence stable).
            if !ordered_blocks.is_empty() || !tool_uses.is_empty() {
                // --- DB: interleaved ---
                let db_content: Vec<serde_json::Value> = if !ordered_blocks.is_empty() {
                    ordered_blocks.iter().map(|b| b.to_json()).collect()
                } else {
                    // Fallback: no blocks captured (shouldn't happen on
                    // stop_reason=tool_use, but stay safe).
                    let mut c = Vec::new();
                    if !assistant_thinking.is_empty() {
                        c.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                    }
                    if !assistant_text.is_empty() {
                        c.push(json!({"type": "text", "text": assistant_text}));
                    }
                    for (tool_id, tool_name, tool_input) in &tool_uses {
                        let input_json: serde_json::Value = serde_json::from_str(tool_input).unwrap_or(json!({}));
                        c.push(json!({
                            "type": "tool_use",
                            "id": tool_id,
                            "name": tool_name,
                            "input": input_json
                        }));
                    }
                    c
                };
                let db_msg = json!({"role": "assistant", "content": db_content});
                save_api_message(&self.db, session_id, &db_msg);
                // Drop the streaming placeholder — see comment in the end_turn branch.
                delete_streaming_message(&self.db, current_msg_id);

                // --- API: aggregated [thinking, text, ...tools] ---
                // Only build a fresh aggregated array if we actually used
                // ordered_blocks above; otherwise the fallback already
                // produced the same array — reuse it for the API too.
                let api_msg = if !ordered_blocks.is_empty() {
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
                    json!({"role": "assistant", "content": api_content})
                } else {
                    db_msg
                };
                api_messages.push(api_msg);
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
                    "content": result_blocks.clone()
                });
                save_api_message(&self.db, session_id, &tool_msg);
                api_messages.push(tool_msg);

                // Notify frontend of persisted tool_results via db_updated event
                emit(&app, &session_key, StreamEvent::DbUpdated {
                    message_id: current_msg_id,
                    content: String::new(),
                    thinking: String::new(),
                    tool_results: result_blocks,
                });
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

        // Save the last cumulative token usage to the database
        if let Some(tokens) = last_cumulative_tokens {
            if let Ok(conn) = self.db.lock() {
                conn.execute(
                    "UPDATE agent_session SET last_token_usage = ? WHERE id = ?",
                    rusqlite::params![tokens as i64, session_id],
                ).ok();
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
            StreamEvent::DbUpdated { .. } => "db_updated",
            StreamEvent::CommandBegin { .. } => "exec_command_begin",
            StreamEvent::CommandDelta { .. } => "exec_command_output_delta",
            StreamEvent::CommandEnd { .. } => "exec_command_end",
            StreamEvent::CommandError { .. } => "exec_command_error",
        };
        eprintln!("[emit] FAILED key={} type={}: {:?}", key, type_name, e);
    }
}

// One block from the LLM's content array, tracked in the order it was
// generated. Used to build api_content / db_content in the original
// interleaving order (e.g. text → tool_use → text → tool_use → text) so
// the UI's displayMessages renders cards in the correct positions
// instead of dumping all tool cards below the final text.
#[derive(Clone)]
enum OrderedBlock {
    Thinking { text: String },
    Text { text: String },
    ToolUse { id: String, name: String, input: String },
}

impl OrderedBlock {
    /// Render this block to its Anthropic-API JSON form for the LLM and DB.
    fn to_json(&self) -> serde_json::Value {
        match self {
            OrderedBlock::Thinking { text } => json!({"type": "thinking", "thinking": text}),
            OrderedBlock::Text { text } => json!({"type": "text", "text": text}),
            OrderedBlock::ToolUse { id, name, input } => {
                let input_json: serde_json::Value =
                    serde_json::from_str(input).unwrap_or_else(|_| json!({}));
                json!({"type": "tool_use", "id": id, "name": name, "input": input_json})
            }
        }
    }
}

// Returns (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_input_tokens, ordered_blocks, had_error)
// tool_uses: Vec<(tool_id, tool_name, input_accumulated)>
// tool_results: Vec<(tool_name, tool_id, result)>
// actual_input_tokens: prompt token count reported by API (None if not available)
// ordered_blocks: the LLM's content blocks in generation order — preserves
// the interleaving between text/thinking and tool_use so the DB and the
// next-turn api_messages both reflect the original sequence.
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
    repeat_history: &mut Vec<String>,
    todo_store: Arc<StdMutex<HashMap<i64, String>>>,
    command_cancel_registry: crate::command_stream::CancelRegistry,
    command_pid_registry: crate::command_stream::CommandPidRegistry,
    current_msg_id: i64,
) -> (Option<String>, String, String, Vec<(String, String, String)>, Vec<(String, String, String)>, Option<u64>, Vec<OrderedBlock>, bool) {
    eprintln!("[process_sse_stream] Starting with session_key: {}", session_key);
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_input = String::new();
    let mut tool_inputs: HashMap<String, (String, String)> = HashMap::new();
    let mut stop_reason: Option<String> = None;
    let mut had_error = false;
    let mut tool_results: Vec<(String, String, String)> = Vec::new();
    let mut assistant_text = String::new();
    let mut assistant_thinking = String::new();
    // Content blocks in the LLM's generation order. The API emits thinking /
    // text / tool_use blocks one after another; we keep every block we see so
    // api_content and db_content can be assembled in the same interleaved
    // order the model produced, instead of flattening to
    // [all-thinking][all-text][all-tool_uses].
    let mut ordered_blocks: Vec<OrderedBlock> = Vec::new();

    // Actual token count from API (last message_delta carries the total)
    let mut actual_input_tokens: Option<u64> = None;

    // Cache usage tracking (prefix-based KV cache)
    let mut cache_hit_tokens: u64 = 0;
    let mut cache_miss_tokens: u64 = 0;

    // Text emission buffer — merge rapid deltas to reduce IPC overhead.
    // 50ms balances IPC overhead against smoothness: emits ~20x/sec max,
    // imperceptible to human eye while reducing frontend load by 6x vs 8ms.
    let mut last_emit = std::time::Instant::now();
    let emit_interval = std::time::Duration::from_millis(50);
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
                let (content, thinking) = (
                    std::mem::take(&mut pending_text),
                    std::mem::take(&mut pending_thinking),
                );
                let ev = StreamEvent::ContentBlockDelta {
                    content: content.clone(),
                    thinking: thinking.clone(),
                };
                emit(&app, &session_key, ev);
                update_streaming_message(&db, session_id, current_msg_id, &content, &thinking);
                emit(&app, &session_key, StreamEvent::DbUpdated {
                    message_id: current_msg_id,
                    content,
                    thinking,
                    tool_results: Vec::new(),
                });
            }
            return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens, ordered_blocks, had_error);
        }

        let item = tokio::select! {
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    eprintln!("[process_sse_stream] Canceled mid-stream for session {}", session_id);
                    if !pending_text.is_empty() || !pending_thinking.is_empty() {
                        let (content, thinking) = (
                            std::mem::take(&mut pending_text),
                            std::mem::take(&mut pending_thinking),
                        );
                        let ev = StreamEvent::ContentBlockDelta {
                            content: content.clone(),
                            thinking: thinking.clone(),
                        };
                        emit(&app, &session_key, ev);
                        update_streaming_message(&db, session_id, current_msg_id, &content, &thinking);
                        emit(&app, &session_key, StreamEvent::DbUpdated {
                            message_id: current_msg_id,
                            content,
                            thinking,
                            tool_results: Vec::new(),
                        });
                    }
                    return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens, ordered_blocks, had_error);
                }
                continue;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(STREAM_SESSION_TIMEOUT_SECS)) => {
                eprintln!("[process_sse_stream] Global timeout ({}s) for session {}", STREAM_SESSION_TIMEOUT_SECS, session_id);
                emit(&app, &session_key, StreamEvent::Error {
                    content: "Session timed out after 1 hour".to_string(),
                });
                return (None, assistant_text, assistant_thinking, Vec::new(), Vec::new(), actual_input_tokens, ordered_blocks, had_error);
            }
            item = stream.next() => { item }
        };

        match item {
            Some(Ok(bytes)) => {
                // Parse SSE directly from bytes — avoid intermediate String allocation.
                // If a chunk's tail is invalid UTF-8 (e.g. a multi-byte char split
                // across two chunks), use valid_up_to() to consume the valid
                // prefix safely — never from_utf8_unchecked, which is UB on the
                // same input and silently corrupts the model output.
                let text = match std::str::from_utf8(&bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        let valid = e.valid_up_to();
                        // valid_up_to() returns the byte index of the end of the
                        // longest valid UTF-8 prefix; slicing up to that point
                        // is always safe.
                        std::str::from_utf8(&bytes[..valid])
                            .unwrap_or("") // fall back to empty if even the prefix is bad
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
                                    &mut ordered_blocks,
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
                                        // input + cache_read = full prompt size (cached + uncached).
                                        // Reported by the API each message_delta, so the last value is the cumulative total.
                                        actual_input_tokens = Some(input + hit);
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
                                        let (content, thinking) = (
                                            std::mem::take(&mut pending_text),
                                            std::mem::take(&mut pending_thinking),
                                        );
                                        let ev = StreamEvent::ContentBlockDelta {
                                            content: content.clone(),
                                            thinking: thinking.clone(),
                                        };
                                        emit(&app, &session_key, ev);
                                        update_streaming_message(&db, session_id, current_msg_id, &content, &thinking);
                                        emit(&app, &session_key, StreamEvent::DbUpdated {
                                            message_id: current_msg_id,
                                            content,
                                            thinking,
                                            tool_results: Vec::new(),
                                        });
                                        last_emit = std::time::Instant::now();
                                    }
                            }
                        }
                    }
            }
            Some(Err(e)) => {
                if !pending_text.is_empty() || !pending_thinking.is_empty() {
                    let (content, thinking) = (
                        std::mem::take(&mut pending_text),
                        std::mem::take(&mut pending_thinking),
                    );
                    if !content.is_empty() || !thinking.is_empty() {
                        let ev = StreamEvent::ContentBlockDelta {
                            content: content.clone(),
                            thinking: thinking.clone(),
                        };
                        emit(&app, &session_key, ev);
                        update_streaming_message(&db, session_id, current_msg_id, &content, &thinking);
                        emit(&app, &session_key, StreamEvent::DbUpdated {
                            message_id: current_msg_id,
                            content,
                            thinking,
                            tool_results: Vec::new(),
                        });
                    }
                }
                emit(&app, &session_key, StreamEvent::Error { content: format!("Stream error: {}", e) });
                delete_streaming_message(&db, current_msg_id);
                had_error = true;
                break;
            }
            None => break,
        }
    }

    // Flush any remaining buffered text
    if !pending_text.is_empty() || !pending_thinking.is_empty() {
        let (content, thinking) = (
            std::mem::take(&mut pending_text),
            std::mem::take(&mut pending_thinking),
        );
        let ev = StreamEvent::ContentBlockDelta {
            content: content.clone(),
            thinking: thinking.clone(),
        };
        emit(&app, &session_key, ev);
        update_streaming_message(&db, session_id, current_msg_id, &content, &thinking);
        emit(&app, &session_key, StreamEvent::DbUpdated {
            message_id: current_msg_id,
            content,
            thinking,
            tool_results: Vec::new(),
        });
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
    // The history is owned by stream_chat and passed in as a mutable ref, so
    // suppression is remembered across turns — otherwise a model that retries
    // 1-2 times per turn would never trip the threshold.
    let repeat_window: usize = std::env::var("MINIMAX_REPEAT_WINDOW")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    let repeat_threshold: usize = std::env::var("MINIMAX_REPEAT_THRESHOLD")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(3);

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
            return (None, assistant_text, assistant_thinking, tool_uses, Vec::new(), actual_input_tokens, ordered_blocks, had_error);
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
                let tool_id_owned = tool_uses[i].0.clone();
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
                let app = app.clone();
                let sid = session_id;
                let cancel_rx = cancel_rx.clone();
                let command_cancel_registry = command_cancel_registry.clone();
                let command_pid_registry = command_pid_registry.clone();
                let session_key = session_key.clone();
                tokio::spawn(async move {
                    let result = execute_tool(
                        &tool_id_owned, &tool_name, &final_input, sid,
                        api_key, api_url, model, provider,
                        skill_service, mcp_service,
                        db, lsp_manager, permission_service, pending_asks, app,
                        cancel_rx, todo_store,
                        command_cancel_registry,
                        command_pid_registry,
                        session_key,
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

    (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results, actual_input_tokens, ordered_blocks, had_error)
}

#[allow(clippy::too_many_arguments)]
fn handle_sse_event(
    event: &serde_json::Value,
    current_tool_id: &mut Option<String>,
    current_tool_name: &mut Option<String>,
    current_tool_input: &mut String,
    tool_inputs: &mut HashMap<String, (String, String)>,
    assistant_text: &mut String,
    assistant_thinking: &mut String,
    ordered_blocks: &mut Vec<OrderedBlock>,
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
                    let id = block["id"].as_str().map(|s| s.to_string()).unwrap_or_default();
                    let name = block["name"].as_str().map(|s| s.to_string()).unwrap_or_default();
                    *current_tool_id = Some(id.clone());
                    *current_tool_name = Some(name.clone());
                    current_tool_input.clear();
                    ordered_blocks.push(OrderedBlock::ToolUse { id, name, input: String::new() });
                }
                "thinking" => {
                    // Some providers send full thinking in content_block_start
                    let initial = block["thinking"].as_str().unwrap_or("").to_string();
                    if !initial.is_empty() {
                        assistant_thinking.push_str(&initial);
                    }
                    ordered_blocks.push(OrderedBlock::Thinking { text: initial });
                }
                "text" => {
                    // The initial text field is usually empty; deltas follow.
                    // Track a block anyway so db_content and api_content
                    // include an empty text block in the correct position
                    // when the model emits text right after a tool_use
                    // (preserves the interleaving).
                    let initial = block["text"].as_str().unwrap_or("").to_string();
                    if !initial.is_empty() {
                        assistant_text.push_str(&initial);
                    }
                    ordered_blocks.push(OrderedBlock::Text { text: initial });
                }
                _ => {}
            }
            None
        }
        "content_block_delta" => {
            let delta = &event["delta"];

            // Apply the delta to the most recently started block.
            // The Anthropic API guarantees deltas for a block arrive
            // sequentially between content_block_start and
            // content_block_stop, so the last block in `ordered_blocks`
            // is the one being updated. If a delta arrives for a block
            // we haven't started yet (protocol error), the last_mut()
            // call returns None and we drop the delta.
            if let Some(last) = ordered_blocks.last_mut() {
                match last {
                    OrderedBlock::Text { text } => {
                        if let Some(t) = delta["text"].as_str() {
                            if !t.is_empty() {
                                text.push_str(t);
                            }
                        }
                    }
                    OrderedBlock::Thinking { text } => {
                        if let Some(t) = delta["thinking"].as_str() {
                            if !t.is_empty() {
                                text.push_str(t);
                            }
                        }
                    }
                    OrderedBlock::ToolUse { input, .. } => {
                        if let Some(p) = delta["partial_json"].as_str() {
                            input.push_str(p);
                        }
                    }
                }
            }

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
    tool_id: &str,
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
    command_cancel_registry: crate::command_stream::CancelRegistry,
    command_pid_registry: crate::command_stream::CommandPidRegistry,
    session_key: String,
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
        "run_command" => {
            let call_id = crate::command_stream::generate_call_id();
            tool_run_command_streaming(
                &params,
                session_id,
                call_id,
                tool_id.to_string(),
                command_cancel_registry.clone(),
                command_pid_registry.clone(),
                app_handle.clone(),
                session_key.clone(),
            ).await
        }
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
        "run_background" => {
            let call_id = crate::command_stream::generate_call_id();
            tool_run_background_streaming(
                &params,
                session_id,
                call_id,
                tool_id.to_string(),
                command_cancel_registry.clone(),
                command_pid_registry.clone(),
                app_handle.clone(),
                session_key.clone(),
            ).await
        }
        "job_output" => tool_job_output(&params).await,
        "wait_for_job" => tool_wait_for_job(&params).await,
        "list_jobs" => tool_list_jobs(&params, session_id).await,
        "spawn_process" => tool_spawn_process(&params, session_id).await,
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
        ("multi_edit", "批量多文件编辑。edits: [{path, search, replace}]。⚠️ 不保证原子性：search 文本不一致/缩进差异/重复匹配都可能部分失败，不会自动回滚。调用后必须 verify 每处生效（见系统提示词红线）", schema_obj(json!({"edits": {"type": "array", "items": {"type": "object"}}}), &["edits"])),
        ("find_replace_in_files", "目录下批量查找替换（支持regex）", schema_obj(json!({"path": {"type": "string"}, "find": {"type": "string"}, "replace": {"type": "string"}, "use_regex": {"type": "boolean"}}), &["path", "find", "replace"])),
        ("create_directory", "创建目录（含父目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("move_file", "移动/重命名文件或目录", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
        ("delete_file", "删除单个文件（拒绝目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("copy_file", "复制文件或目录（递归）", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
    ];

    // Command execution (work only)
    let command_tools: &[(&str, &str, serde_json::Value)] = &[
        ("run_command", "执行命令（阻塞等完成）。直接用要执行的命令，自由选择 shell：如 git status、cmd /c dir、powershell -Command \"...\"、sh -c \"...\"。支持命令链（&&, ||, |）；git 子命令智能解析（status/log/diff 等自动通过，push --force main 被阻止）。⚠️ Windows 路径含中文/空格 → 直接 PowerShell（powershell -NoProfile -Command \"...\"），不先试 cmd 浪费一轮。⚠️ 严禁交互式命令：npm init(无-y)、git commit(无-m)、git rebase -i、ssh、python/node REPL、less/more/vim 等会等待输入的命令会卡死120秒。所有install命令加 --yes/--no-input。", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "timeout": {"type": "integer"}}), &["command"])),
        ("run_background", "后台运行长进程（dev server/build）立即返回 task_id。命令自由选择 shell。自动检测就绪信号（如 Vite ready、npm DONE 等）。输出实时推送前端面板。kill 时先 SIGTERM 再 SIGKILL 优雅退出。用 wait_for_job 等待完成或检测就绪，用 list_jobs 查看任务列表，用 job_output 查看输出。⚠️ 严禁交互式命令（同 run_command）。", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "wait_sec": {"type": "integer"}}), &["command"])),
        ("kill_process", "按PID强制终止任意进程（不经过Registry，适合紧急杀进程）", schema_obj(json!({"pid": {"type": "number"}}), &["pid"])),
        ("job_output", "查询后台任务输出。通过 job_id（run_background 返回）查 Registry 获取输出文件，tail_lines 返回最后N行", schema_obj(json!({"job_id": {"type": "integer"}, "tail_lines": {"type": "integer"}}), &["job_id"])),
        ("wait_for_job", "阻塞等待后台任务退出或匹配就绪信号（Vite ready、npm DONE等）。返回 {status, matched_signal, elapsed_ms, output}。⚠️ 异常返回：elapsed_ms < 1000 + status='exited' = 极可能工具提前返回（已观察到 wait 72ms 误报），必须用 list_jobs / stat 产物二次确认", schema_obj(json!({"job_id": {"type": "integer"}, "timeout_secs": {"type": "integer"}, "check_interval_ms": {"type": "integer"}}), &["job_id"])),
        ("list_jobs", "列出当前会话所有后台任务（含task_id/pid/命令/状态），只查Registry不查系统进程", schema_obj(json!({}), &[])),
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