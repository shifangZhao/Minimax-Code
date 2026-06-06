// Background task registry + output polling for frontend visualization.
//
// tool_run_background registers every spawned process here. A global poller
// thread reads output files every 2s and emits `background_task_output`
// events to the frontend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tauri::{AppHandle, Emitter};
use serde::Serialize;

/// Well-known ready signal patterns for common dev servers and tools.
/// (pattern, human-readable label)
const READY_SIGNALS: &[(&str, &str)] = &[
    ("listening on", "HTTP server ready"),
    ("started server", "Dev server ready"),
    ("ready on", "Server ready"),
    ("Application started", "Java/Ktor ready"),
    ("Server is running", "Server running"),
    ("Vite", "Vite dev server"),
    ("webpack", "Webpack ready"),
    ("✅", "Success emoji"),
    (" DONE ", "npm Done"),
    ("Hot Reload", "React/Vite HR"),
    ("Compiling", "Compiling"),
    ("Local:", "Next.js ready"),
    ("Ready in", "Dev server ready"),
    ("Built at", "Build complete"),
    ("Starting server", "Server starting"),
    ("Server started", "Server started"),
];

/// Check if output contains any ready signal pattern.
/// Returns Some(label) on first match, None otherwise.
pub(crate) fn detect_ready_signal(output: &str) -> Option<&'static str> {
    for (pattern, label) in READY_SIGNALS {
        if output.contains(pattern) {
            return Some(*label);
        }
    }
    None
}

/// Unique task id (incrementing counter).
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackgroundTask {
    pub id: u64,
    pub session_id: i64,
    pub pid: u32,
    pub command: String,
    pub out_file: String,
    pub err_file: String,
    pub start_time: u64,
    pub running: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskOutputEvent {
    pub task_id: u64,
    pub session_id: i64,
    pub pid: u32,
    pub command: String,
    #[serde(rename = "type")]
    pub event_type: String, // "started" | "output" | "exited"
    pub output_delta: String,
    pub out_file: String,
    pub start_time: u64,
    pub exit_code: Option<i32>,
}

struct Registry {
    tasks: HashMap<u64, BackgroundTask>,
    /// Per-task output cursor: byte offset already emitted to frontend.
    cursors: HashMap<u64, u64>,
}

static REGISTRY: OnceLock<Arc<Mutex<Registry>>> = OnceLock::new();
static POLLER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Stored AppHandle so register_task can emit events immediately.
static APP: OnceLock<AppHandle> = OnceLock::new();

fn registry() -> &'static Arc<Mutex<Registry>> {
    REGISTRY.get_or_init(|| {
        Arc::new(Mutex::new(Registry {
            tasks: HashMap::new(),
            cursors: HashMap::new(),
        }))
    })
}

/// Returns the temp directory for background task output files.
/// Uses a unique subdirectory that we create and clean on startup.
fn tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir()
        .join("minimaxcode")
        .join("bg_tasks");
    // Ensure directory exists
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Generate unique output file paths using a ULID-like timestamp+random suffix.
/// This avoids collisions when multiple sessions spawn tasks at the same ms.
pub(crate) fn make_output_files(session_id: i64, _command: &str) -> (PathBuf, PathBuf) {
    let base = format!(
        "s{}_p{}_{}",
        session_id,
        std::process::id(),
        hex_timestamp_nanos()
    );
    let out = tmp_dir().join(format!("{}_out.txt", base));
    let err = tmp_dir().join(format!("{}_err.txt", base));
    (out, err)
}

/// Returns a hex string of the current timestamp at nanosecond resolution.
fn hex_timestamp_nanos() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}

/// Register a newly spawned background process. Returns the task id.
/// Immediately emits a "started" event so the frontend panel appears
/// without waiting for the 2s poller cycle or first output byte.
pub(crate) fn register_task(
    session_id: i64,
    pid: u32,
    command: &str,
    out_file: &str,
    err_file: &str,
) -> u64 {
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let task = BackgroundTask {
        id,
        session_id,
        pid,
        command: command.to_string(),
        out_file: out_file.to_string(),
        err_file: err_file.to_string(),
        start_time,
        running: true,
        exit_code: None,
    };
    let mut reg = registry().lock().unwrap();
    reg.cursors.insert(id, 0);
    reg.tasks.insert(id, task);

    // Register with unified process manager
    crate::process_manager::register_process(
        pid,
        crate::process_manager::ProcessType::Background,
        command,
        Some(session_id),
        None,
        Some(id),
    );

    // Emit "started" event immediately — frontend needs to know about this
    // task right away, not after the next 2s poller cycle.
    if let Some(app) = APP.get() {
        let event = TaskOutputEvent {
            task_id: id,
            session_id,
            pid,
            command: command.to_string(),
            event_type: "started".to_string(),
            output_delta: String::new(),
            out_file: out_file.to_string(),
            start_time,
            exit_code: None,
        };
        let _ = app.emit("background_task_output", &event);
    }

    id
}

/// Mark a task as done (called by reaper thread in agent_tools).
pub(crate) fn task_done(task_id: u64, exit_code: Option<i32>) {
    let mut reg = registry().lock().unwrap();
    if let Some(task) = reg.tasks.get_mut(&task_id) {
        task.running = false;
        task.exit_code = exit_code;
        crate::process_manager::mark_exited(task.pid, exit_code);
    }
}

/// Remove a task from the registry entirely (used when the user dismisses it
/// from the frontend). Does NOT terminate the process if still running —
/// use `kill_task` for that. Safe to call for done tasks; cleans up the
/// cursor entry too.
pub(crate) fn remove_task(task_id: u64) {
    let mut reg = registry().lock().unwrap();
    reg.tasks.remove(&task_id);
    reg.cursors.remove(&task_id);
}

/// Check if a process is alive by PID (public API for use by agent_tools).
pub(crate) fn is_task_alive(pid: u32) -> bool {
    is_process_alive(pid)
}

/// Get task info by task_id. Returns (pid, out_file, running, exit_code).
pub(crate) fn get_task(task_id: u64) -> Option<(u32, String, bool, Option<i32>)> {
    let reg = registry().lock().unwrap();
    reg.tasks.get(&task_id).map(|t| (t.pid, t.out_file.clone(), t.running, t.exit_code))
}

/// List tasks for a session.
pub(crate) fn list_tasks(session_id: i64) -> Vec<BackgroundTask> {
    registry()
        .lock()
        .unwrap()
        .tasks
        .values()
        .filter(|t| t.session_id == session_id)
        .cloned()
        .collect()
}

/// Read output file and return the last `tail_lines` lines.
/// Uses efficient file seeking instead of reading entire file.
pub(crate) fn read_output(out_file: &str, tail_lines: usize) -> String {
    let path = PathBuf::from(out_file);
    if !path.exists() {
        return String::new();
    }

    // Try to use `tail` command if available (most efficient)
    #[cfg(windows)]
    let result = {
        let output = crate::agent_service::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Get-Content '{}' -Tail {} -Encoding utf8",
                    out_file.replace('\'', "''"),
                    tail_lines
                ),
            ])
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).into_owned();
                if s.is_empty() { fallback_read_output(out_file, tail_lines) } else { s }
            }
            Err(_) => fallback_read_output(out_file, tail_lines),
        }
    };

    #[cfg(not(windows))]
    let result = {
        let output = crate::agent_service::hidden_cmd("tail")
            .args(["-n", &tail_lines.to_string(), out_file])
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).into_owned();
                if s.is_empty() { fallback_read_output(out_file, tail_lines) } else { s }
            }
            Err(_) => fallback_read_output(out_file, tail_lines),
        }
    };

    result
}

/// Fallback: read file and extract last N lines manually.
/// Used when `tail` command is unavailable.
fn fallback_read_output(out_file: &str, tail_lines: usize) -> String {
    let bytes = match std::fs::read(out_file) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(tail_lines);
    lines[start..].join("\n")
}

/// Kill a background task gracefully: SIGTERM first, then SIGKILL if still alive after 2s.
pub(crate) fn kill_task(task_id: u64) -> Result<(), String> {
    let (pid, out_file, err_file) = {
        let reg = registry().lock().unwrap();
        reg.tasks.get(&task_id).map(|t| (t.pid, t.out_file.clone(), t.err_file.clone()))
    }.ok_or_else(|| format!("Task {} not found", task_id))?;

    #[cfg(windows)]
    {
        // Graceful: send WM_CLOSE to the process tree
        let result = crate::agent_service::hidden_cmd("taskkill")
            .args(["/T", "/PID", &pid.to_string()])
            .output();
        match result {
            Ok(_) => {
                thread::sleep(std::time::Duration::from_secs(3));
                if is_process_alive(pid) {
                    crate::agent_service::hidden_cmd("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output()
                        .map_err(|e| e.to_string())?;
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    #[cfg(not(windows))]
    {
        crate::agent_service::hidden_cmd("kill")
            .args(["-15", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;

        thread::sleep(std::time::Duration::from_secs(3));

        if is_process_alive(pid) {
            crate::agent_service::hidden_cmd("kill")
                .args(["-9", &pid.to_string()])
                .output()
                .map_err(|e| e.to_string())?;
        }
    }
    task_done(task_id, Some(-1));

    // Clean up temp files
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(&err_file);

    Ok(())
}

/// Start the global poller thread (idempotent — only starts once).
pub(crate) fn start_poller(app: AppHandle) {
    let _ = APP.set(app.clone());
    if POLLER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        // Clean up stale temp files from previous runs on startup
        cleanup_stale_temp_files();
        // Track file sizes to avoid reading when no new data
        let mut file_sizes: HashMap<u64, u64> = HashMap::new();
        loop {
            thread::sleep(std::time::Duration::from_millis(1000)); // Increased from 500ms
            let snap: Vec<(u64, i64, u32, PathBuf, String, u64)> = {
                let reg = registry().lock().unwrap();
                reg.tasks
                    .values()
                    .filter(|t| t.running)
                    .map(|t| {
                        (
                            t.id,
                            t.session_id,
                            t.pid,
                            PathBuf::from(&t.out_file),
                            t.command.clone(),
                            t.start_time,
                        )
                    })
                    .collect()
            };
            for (id, session_id, pid, out_file, command, start_time) in &snap {
                if !out_file.exists() {
                    continue;
                }

                // Read only new bytes since last cursor
                let total_bytes = match std::fs::metadata(out_file) {
                    Ok(m) => m.len(),
                    Err(_) => continue,
                };

                // Skip if file size hasn't changed since last check
                let last_size = file_sizes.get(id).copied().unwrap_or(0);
                if total_bytes == last_size {
                    continue;
                }
                file_sizes.insert(*id, total_bytes);

                let delta = {
                    let mut reg = registry().lock().unwrap();
                    let cursor = reg.cursors.get(id).copied().unwrap_or(0);
                    if total_bytes > cursor {
                        // Seek and read only new bytes — O(new bytes) not O(total file)
                        let mut file = match std::fs::File::open(out_file) {
                            Ok(f) => f,
                            Err(_) => continue,
                        };
                        use std::io::Seek;
                        if file.seek(std::io::SeekFrom::Start(cursor)).is_err() {
                            continue;
                        }
                        let mut buf = Vec::new();
                        buf.resize((total_bytes - cursor) as usize, 0);
                        let actual = std::io::Read::read(&mut file, &mut buf).unwrap_or(0);
                        buf.truncate(actual);
                        reg.cursors.insert(*id, total_bytes);
                        String::from_utf8_lossy(&buf).into_owned()
                    } else {
                        String::new()
                    }
                };

                if !delta.is_empty() {
                    let event = TaskOutputEvent {
                        task_id: *id,
                        session_id: *session_id,
                        pid: *pid,
                        command: command.clone(),
                        event_type: "output".to_string(),
                        output_delta: delta,
                        out_file: out_file.to_string_lossy().into_owned(),
                        start_time: *start_time,
                        exit_code: None,
                    };
                    let _ = app.emit("background_task_output", &event);
                }

                // Check if process is still alive using non-blocking wait
                if !is_process_alive(*pid) {
                    // Final read of all remaining output
                    let final_output = {
                        let total = std::fs::metadata(out_file).map(|m| m.len()).unwrap_or(0);
                        let mut reg = registry().lock().unwrap();
                        let cursor = reg.cursors.get(id).copied().unwrap_or(0);
                        if total > cursor {
                            let mut file = match std::fs::File::open(out_file) {
                                Ok(f) => f,
                                Err(_) => {
                                    reg.cursors.insert(*id, total);
                                    return;
                                }
                            };
                            use std::io::Seek;
                            if file.seek(std::io::SeekFrom::Start(cursor)).is_err() {
                                reg.cursors.insert(*id, total);
                                return;
                            }
                            let mut buf = Vec::new();
                            buf.resize((total - cursor) as usize, 0);
                            let actual = std::io::Read::read(&mut file, &mut buf).unwrap_or(0);
                            buf.truncate(actual);
                            reg.cursors.insert(*id, total);
                            String::from_utf8_lossy(&buf).into_owned()
                        } else {
                            String::new()
                        }
                    };

                    {
                        let mut reg = registry().lock().unwrap();
                        if let Some(t) = reg.tasks.get_mut(id) {
                            t.running = false;
                        }
                    }

                    let event = TaskOutputEvent {
                        task_id: *id,
                        session_id: *session_id,
                        pid: *pid,
                        command: command.clone(),
                        event_type: "exited".to_string(),
                        output_delta: final_output,
                        out_file: out_file.to_string_lossy().into_owned(),
                        start_time: *start_time,
                        exit_code: None,
                    };
                    let _ = app.emit("background_task_output", &event);

                    // Clean up temp files and size tracking after process exits
                    file_sizes.remove(id);
                    let err_file = {
                        let reg = registry().lock().unwrap();
                        reg.tasks.get(id).map(|t| t.err_file.clone())
                    };
                    if let Some(err) = err_file {
                        let _ = std::fs::remove_file(&err);
                    }
                    let _ = std::fs::remove_file(out_file);
                }
            }
        }
    });
}

/// Clean up temp files from previous runs (older than 24 hours).
fn cleanup_stale_temp_files() {
    let dir = tmp_dir();
    if !dir.exists() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    let age_secs = std::time::SystemTime::now()
                        .duration_since(modified)
                        .unwrap()
                        .as_secs();
                    if age_secs > 86400 {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        // Use tasklist with /FI filter — more reliable than parsing output
        let output = crate::agent_service::hidden_cmd("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).into_owned();
                // CSV format: "Image Name","PID","Session Name","Session#","Mem Usage"
                s.lines().any(|line| {
                    let fields: Vec<&str> = line.split(',').collect();
                    fields.get(1).map(|f| f.trim().trim_matches('"') == pid.to_string()).unwrap_or(false)
                })
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        crate::agent_service::hidden_cmd("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout).into_owned();
                s.trim().parse::<u32>().ok() == Some(pid)
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_tmp_files() -> (PathBuf, PathBuf) {
        let tmp = std::env::temp_dir().join("minimaxcode").join("bg_tasks");
        let _ = std::fs::create_dir_all(&tmp);
        let out = tmp.join(format!("test_out_{}.txt", std::process::id()));
        let err = tmp.join(format!("test_err_{}.txt", std::process::id()));
        (out, err)
    }

    fn register_fake(session_id: i64, pid: u32, cmd: &str) -> u64 {
        let (out, err) = fake_tmp_files();
        register_task(session_id, pid, cmd, &out.to_string_lossy(), &err.to_string_lossy())
    }

    #[test]
    fn register_and_list() {
        let id = register_fake(1, 9999, "echo test");
        assert!(id > 0);
        let tasks = list_tasks(1);
        assert!(tasks.iter().any(|t| t.id == id));
        assert!(tasks.iter().any(|t| t.pid == 9999));
        assert!(tasks.iter().any(|t| t.command == "echo test"));
        assert!(tasks.iter().any(|t| t.running));
    }

    #[test]
    fn task_done_marks_not_running() {
        let id = register_fake(2, 8888, "sleep 1");
        task_done(id, Some(0));
        let tasks = list_tasks(2);
        let t = tasks.iter().find(|t| t.id == id).unwrap();
        assert!(!t.running);
        assert_eq!(t.exit_code, Some(0));
    }

    #[test]
    fn list_tasks_filters_by_session() {
        let id1 = register_fake(10, 1001, "cmd1");
        let id2 = register_fake(20, 2002, "cmd2");

        let s10 = list_tasks(10);
        assert!(s10.iter().any(|t| t.id == id1));
        assert!(!s10.iter().any(|t| t.id == id2));

        let s20 = list_tasks(20);
        assert!(!s20.iter().any(|t| t.id == id1));
        assert!(s20.iter().any(|t| t.id == id2));
    }

    #[test]
    fn list_empty_session() {
        let tasks = list_tasks(99999);
        assert!(tasks.is_empty());
    }

    #[test]
    fn multiple_tasks_same_session() {
        let id1 = register_fake(30, 3001, "cmd A");
        let id2 = register_fake(30, 3002, "cmd B");
        let tasks = list_tasks(30);
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.id == id1));
        assert!(tasks.iter().any(|t| t.id == id2));
    }

    #[test]
    fn read_output_file() {
        let (out, _err) = fake_tmp_files();
        std::fs::write(&out, "line1\nline2\nline3\nline4\nline5").unwrap();
        let result = read_output(&out.to_string_lossy(), 3);
        assert_eq!(result.lines().count(), 3);
        assert!(result.contains("line3"));
        assert!(result.contains("line5"));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn read_output_tail_more_than_content() {
        let (out, _err) = fake_tmp_files();
        std::fs::write(&out, "only\n").unwrap();
        let result = read_output(&out.to_string_lossy(), 100);
        assert_eq!(result, "only");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn read_output_missing_file() {
        let result = read_output("/nonexistent/bg_file_12345.txt", 50);
        assert!(result.is_empty());
    }

    #[test]
    fn kill_nonexistent_task() {
        let result = kill_task(999999);
        assert!(result.is_err());
    }

    #[test]
    fn make_output_files_unique_per_call() {
        let (out1, err1) = make_output_files(1, "test");
        let (out2, err2) = make_output_files(1, "test");
        assert_ne!(out1, out2, "ULID-like naming should produce unique paths");
        assert_ne!(err1, err2);
    }
}