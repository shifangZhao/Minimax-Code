// Streaming command execution — unified protocol for run_command / run_background.
//
// 协议(NDJSON-style,通过 Tauri `agent_stream_${sessionId}` 通道推送):
//   exec_command_begin    { call_id, tool_id, command, cwd }
//   exec_command_output_delta { call_id, tool_id, stream: "stdout"|"stderr", chunk_b64 }
//   exec_command_end      { call_id, tool_id, exit_code, killed, truncated }
//   exec_command_error    { call_id, tool_id, error }
//
// 关键设计:底层始终流式;阻塞消费端(如 lib.rs:run_command)显式聚合;流式消费端
// (前端 ToolCard)实时 push。事件携带 `call_id` 支持并发命令乱序回传与按 call_id 取消。

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::oneshot;

use crate::agent_service::async_hidden_cmd;

/// Correlate begin/delta/end events for one command invocation.
pub type CallId = String;

/// Global cancel registry: call_id -> oneshot::Sender.
/// Drop semantics guarantee we only fire once even if a natural exit races.
pub type CancelRegistry = Arc<StdMutex<HashMap<CallId, oneshot::Sender<()>>>>;

/// Per-(session, call_id) PID tracking for targeted abort_command.
pub type CommandPidRegistry = Arc<StdMutex<HashMap<(i64, CallId), u32>>>;

/// NDJSON-style events emitted to the frontend. Each event carries
/// `call_id` so concurrent commands can be deinterleaved by consumers.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum CommandStreamEvent {
    #[serde(rename = "exec_command_begin")]
    Begin {
        call_id: CallId,
        tool_id: String,
        command: String,
        cwd: Option<String>,
    },
    #[serde(rename = "exec_command_output_delta")]
    Delta {
        call_id: CallId,
        tool_id: String,
        stream: &'static str, // "stdout" | "stderr"
        chunk_b64: String,
    },
    #[serde(rename = "exec_command_end")]
    End {
        call_id: CallId,
        tool_id: String,
        exit_code: Option<i32>,
        killed: bool,
        truncated: bool,
    },
    #[serde(rename = "exec_command_error")]
    Error {
        call_id: CallId,
        tool_id: String,
        error: String,
    },
}

/// RAII guard: deregister call_id from cancel registry and pid registry on drop,
/// so any panic / early return cannot leak stale entries.
struct CommandGuard {
    cancel_registry: CancelRegistry,
    command_pid_registry: CommandPidRegistry,
    session_id: i64,
    call_id: CallId,
    pid: u32,
}

impl Drop for CommandGuard {
    fn drop(&mut self) {
        crate::process_manager::mark_exited(self.pid, None);
        if let Ok(mut reg) = self.cancel_registry.lock() {
            reg.remove(&self.call_id);
        }
        if let Ok(mut reg) = self.command_pid_registry.lock() {
            reg.remove(&(self.session_id, self.call_id.clone()));
        }
    }
}

fn force_kill(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // Graceful: taskkill /T sends WM_CLOSE to the process tree
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        std::thread::sleep(std::time::Duration::from_secs(3));
        // Force: taskkill /F /T terminates immediately
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-15", &pid.to_string()])
            .output();
        std::thread::sleep(std::time::Duration::from_secs(3));
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// Spawn a child via async_hidden_cmd, then stream stdout/stderr chunks to the
/// frontend as `Delta` events. Aggregated output + exit code returned for the
/// LLM's tool_result (capped at MAX_OUTPUT bytes per stream, UTF-8 safe).
///
/// This is the unified streaming core: every run_command / run_background goes
/// through here. Blocking consumers can simply ignore the Delta events and use
/// the returned aggregate.
#[allow(clippy::too_many_arguments)]
pub async fn async_run_streaming(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    timeout_secs: u64,
    session_id: i64,
    call_id: CallId,
    tool_id: String,
    cancel_registry: CancelRegistry,
    command_pid_registry: CommandPidRegistry,
    app: AppHandle,
    session_key: String,
) -> (String, String, Option<i32>, bool, bool, usize) {
    let mut cmd: TokioCommand = async_hidden_cmd(program);
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_event(
                &app,
                &session_key,
                CommandStreamEvent::Error {
                    call_id: call_id.clone(),
                    tool_id: tool_id.clone(),
                    error: format!("Error spawning command: {}", e),
                },
            );
            return (String::new(), String::new(), None, false, false, 0);
        }
    };

    let pid = child.id().unwrap_or(0);

    // Register with unified process manager
    crate::process_manager::register_process(
        pid,
        crate::process_manager::ProcessType::Command,
        &format!("{} {}", program, args.join(" ")),
        Some(session_id),
        Some(call_id.clone()),
        None,
    );

    // Register cancel sender + pid
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    {
        if let Ok(mut reg) = cancel_registry.lock() {
            reg.insert(call_id.clone(), cancel_tx);
        }
    }
    if let Ok(mut reg) = command_pid_registry.lock() {
        reg.insert((session_id, call_id.clone()), pid);
    }

    let _guard = CommandGuard {
        cancel_registry: cancel_registry.clone(),
        command_pid_registry: command_pid_registry.clone(),
        session_id,
        call_id: call_id.clone(),
        pid,
    };

    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    const MAX_OUTPUT: usize = 30_000; // 30K chars cap — keeps LLM context compact
    const READ_BUF: usize = 8 * 1024; // 8KB read chunk — balances latency vs IPC overhead

    let stdout_bytes_arc = Arc::new(StdMutex::new(Vec::<u8>::with_capacity(8192)));
    let stderr_bytes_arc = Arc::new(StdMutex::new(Vec::<u8>::with_capacity(4096)));
    let total_out_arc = Arc::new(StdMutex::new(0usize));
    let total_err_arc = Arc::new(StdMutex::new(0usize));

    // Spawn stdout reader
    let app_out = app.clone();
    let session_key_out = session_key.clone();
    let call_id_out = call_id.clone();
    let tool_id_out = tool_id.clone();
    let stdout_bytes = stdout_bytes_arc.clone();
    let total_out = total_out_arc.clone();
    let out_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; READ_BUF];
        let Some(mut stdout) = child_stdout else { return };
        loop {
            let n = match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = &buf[..n];
            let (to_emit, should_break) = {
                let mut total = total_out.lock().unwrap();
                if *total >= MAX_OUTPUT {
                    (Vec::new(), true)
                } else {
                    let remaining = MAX_OUTPUT - *total;
                    let to_take = n.min(remaining);
                    stdout_bytes.lock().unwrap().extend_from_slice(&chunk[..to_take]);
                    *total += to_take;
                    (chunk[..to_take].to_vec(), to_take < n)
                }
            };
            if !to_emit.is_empty() {
                let b64 = B64.encode(&to_emit);
                emit_event(
                    &app_out,
                    &session_key_out,
                    CommandStreamEvent::Delta {
                        call_id: call_id_out.clone(),
                        tool_id: tool_id_out.clone(),
                        stream: "stdout",
                        chunk_b64: b64,
                    },
                );
            }
            if should_break {
                break;
            }
        }
    });

    // Spawn stderr reader
    let app_err = app.clone();
    let session_key_err = session_key.clone();
    let call_id_err = call_id.clone();
    let tool_id_err = tool_id.clone();
    let stderr_bytes = stderr_bytes_arc.clone();
    let total_err = total_err_arc.clone();
    let err_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; READ_BUF];
        let Some(mut stderr) = child_stderr else { return };
        loop {
            let n = match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = &buf[..n];
            let (to_emit, should_break) = {
                let mut total = total_err.lock().unwrap();
                if *total >= MAX_OUTPUT {
                    (Vec::new(), true)
                } else {
                    let remaining = MAX_OUTPUT - *total;
                    let to_take = n.min(remaining);
                    stderr_bytes.lock().unwrap().extend_from_slice(&chunk[..to_take]);
                    *total += to_take;
                    (chunk[..to_take].to_vec(), to_take < n)
                }
            };
            if !to_emit.is_empty() {
                let b64 = B64.encode(&to_emit);
                emit_event(
                    &app_err,
                    &session_key_err,
                    CommandStreamEvent::Delta {
                        call_id: call_id_err.clone(),
                        tool_id: tool_id_err.clone(),
                        stream: "stderr",
                        chunk_b64: b64,
                    },
                );
            }
            if should_break {
                break;
            }
        }
    });

    // Race: process exit vs cancel vs timeout

    let (exit_code, killed) = tokio::select! {
        result = child.wait() => {
            match result {
                Ok(status) => (status.code(), false),
                Err(_) => (None, false),
            }
        }
        _ = cancel_rx => {
            force_kill(pid);
            let _ = child.kill().await;
            (None, true)
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs)) => {
            force_kill(pid);
            let _ = child.kill().await;
            // Brief wait for OS pipe buffers to flush
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            (None, true)
        }
    };

    // Drain readers with bounded wait
    let drain_timeout = if killed && exit_code.is_none() {
        std::time::Duration::from_secs(2)
    } else {
        std::time::Duration::from_secs(10)
    };
    let _ = tokio::time::timeout(drain_timeout, out_handle).await;
    let _ = tokio::time::timeout(drain_timeout, err_handle).await;

    // Decode + UTF-8 safe truncation (reuse existing semantics)
    let stdout_bytes_vec = stdout_bytes_arc.lock().unwrap().clone();
    let stderr_bytes_vec = stderr_bytes_arc.lock().unwrap().clone();
    let stdout_total = *total_out_arc.lock().unwrap();
    let stderr_total = *total_err_arc.lock().unwrap();

    let (stdout, stdout_truncated) = decode_output_truncated(&stdout_bytes_vec, MAX_OUTPUT);
    let stderr = decode_output_bytes(&stderr_bytes_vec);
    let stderr_truncated = stderr_total > MAX_OUTPUT;
    let truncated = stdout_truncated || stderr_truncated;
    let total_output_len = stdout_total + stderr_total;

    emit_event(
        &app,
        &session_key,
        CommandStreamEvent::End {
            call_id: call_id.clone(),
            tool_id: tool_id.clone(),
            exit_code,
            killed,
            truncated,
        },
    );

    drop(_guard); // deregister

    (stdout, stderr, exit_code, killed, truncated, total_output_len)
}

fn emit_event(app: &AppHandle, session_key: &str, event: CommandStreamEvent) {
    if let Err(e) = app.emit(session_key, &event) {
        eprintln!("[command_stream] emit FAILED: {:?}", e);
    }
}

fn decode_output_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_output_truncated(bytes: &[u8], max_len: usize) -> (String, bool) {
    if bytes.len() <= max_len {
        return (decode_output_bytes(bytes), false);
    }
    let mut end = max_len;
    while end > 0 && (bytes[end] & 0x80) != 0 && (bytes[end] & 0xC0) != 0xC0 {
        end -= 1;
    }
    if end == 0 {
        end = max_len;
    }
    let head = decode_output_bytes(&bytes[..end]);
    (head, true)
}

// ── ID generation (no uuid dep) ──

static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn generate_call_id() -> CallId {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = CALL_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("call_{:x}_{:x}", nanos, counter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_ids_are_unique() {
        let a = generate_call_id();
        let b = generate_call_id();
        let c = generate_call_id();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert!(a.starts_with("call_"));
    }

    #[test]
    fn decode_truncates_at_utf8_boundary() {
        // 4-byte UTF-8 char (U+1F600 = 😀) is 0xF0 0x9F 0x98 0x80
        let mut bytes = vec![b'a'; 10];
        bytes.extend_from_slice(&[0xF0, 0x9F, 0x98, 0x80]); // emoji at end
        bytes.extend_from_slice(&[b'b'; 10]);
        // Cap lands mid-emoji (byte 10..14). Backward boundary scan drops
        // the partial sequence to keep output always-valid UTF-8.
        let (s, truncated) = decode_output_truncated(&bytes, 12);
        assert!(truncated);
        // Should be 10 'a' bytes — the partial emoji was safely dropped
        assert_eq!(s, "aaaaaaaaaa");
        // The bytes that follow the boundary (10 b's) are excluded by the cap.
        assert!(!s.contains('b'));
    }

    #[test]
    fn decode_preserves_complete_utf8_at_boundary() {
        // 4-byte UTF-8 char fits exactly before the cap
        let mut bytes = vec![b'a'; 10];
        bytes.extend_from_slice(&[0xF0, 0x9F, 0x98, 0x80]); // emoji at [10..14)
        // Cap at 14 → should keep all 14 bytes (10 a's + emoji) and not truncate
        let (s, truncated) = decode_output_truncated(&bytes, 14);
        assert!(!truncated);
        assert!(s.starts_with("aaaaaaaaaa"));
        assert!(s.contains("\u{1F600}"));
    }

    #[test]
    fn decode_under_cap_is_not_truncated() {
        let bytes = b"hello world".to_vec();
        let (s, truncated) = decode_output_truncated(&bytes, 1024);
        assert_eq!(s, "hello world");
        assert!(!truncated);
    }

    #[test]
    fn registry_round_trip() {
        let reg: CancelRegistry = Arc::new(StdMutex::new(HashMap::new()));
        let call_id = generate_call_id();
        let (tx, rx) = oneshot::channel::<()>();
        reg.lock().unwrap().insert(call_id.clone(), tx);
        let tx = reg.lock().unwrap().remove(&call_id).unwrap();
        tx.send(()).unwrap();
        let _ = rx.blocking_recv();
        assert!(reg.lock().unwrap().is_empty());
    }

    #[test]
    fn pid_registry_cleanup() {
        let reg: CommandPidRegistry = Arc::new(StdMutex::new(HashMap::new()));
        let call_id = generate_call_id();
        reg.lock()
            .unwrap()
            .insert((42, call_id.clone()), 1234u32);
        assert!(reg.lock().unwrap().contains_key(&(42, call_id.clone())));
        reg.lock().unwrap().remove(&(42, call_id.clone()));
        assert!(reg.lock().unwrap().is_empty());
    }

    #[test]
    fn counter_advances() {
        let a = generate_call_id();
        let b = generate_call_id();
        assert_ne!(a, b);
    }
}
