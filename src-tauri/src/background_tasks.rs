// Background task registry + output polling for frontend visualization.
//
// tool_run_background registers every spawned process here. A global poller
// thread reads output files every 2s and emits `background_task_output`
// events to the frontend.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use serde::Serialize;

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
    pub event_type: String,  // "started" | "output" | "exited"
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
    }
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

/// Read output file and return full content (lossy UTF-8 for Windows GBK safety).
pub(crate) fn read_output(out_file: &str, tail_lines: usize) -> String {
    let bytes = std::fs::read(out_file).unwrap_or_default();
    let content = String::from_utf8_lossy(&bytes).to_string();
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start = total.saturating_sub(tail_lines);
    lines[start..].join("\n")
}

/// Kill a background task by PID.
pub(crate) fn kill_task(task_id: u64) -> Result<(), String> {
    let pid = {
        let reg = registry().lock().unwrap();
        reg.tasks.get(&task_id).map(|t| t.pid)
    };
    match pid {
        Some(pid) => {
            #[cfg(windows)]
            {
                crate::agent_service::hidden_cmd("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output()
                    .map_err(|e| e.to_string())?;
            }
            #[cfg(not(windows))]
            {
                crate::agent_service::hidden_cmd("kill")
                    .args(["-9", &pid.to_string()])
                    .output()
                    .map_err(|e| e.to_string())?;
            }
            task_done(task_id, Some(-1));
            Ok(())
        }
        None => Err(format!("Task {} not found", task_id)),
    }
}

/// Start the global poller thread (idempotent — only starts once).
pub(crate) fn start_poller(app: AppHandle) {
    // Store AppHandle so register_task can emit events immediately
    let _ = APP.set(app.clone());
    if POLLER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let snap: Vec<(u64, u64, u32, String, String, u64)> = {
            let reg = registry().lock().unwrap();
            reg.tasks
                .values()
                .filter(|t| t.running)
                .map(|t| (t.id, t.session_id as u64, t.pid, t.out_file.clone(), t.command.clone(), t.start_time))
                .collect()
        };
        for (id, session_id, pid, out_file, command, start_time) in &snap {
            let bytes = std::fs::read(out_file).unwrap_or_default();
            let content = String::from_utf8_lossy(&bytes).to_string();
            let total_bytes = content.len() as u64;

            let delta = {
                let mut reg = registry().lock().unwrap();
                let cursor = reg.cursors.get(id).copied().unwrap_or(0);
                if total_bytes > cursor {
                    let delta_bytes = &content[cursor as usize..];
                    reg.cursors.insert(*id, total_bytes);
                    delta_bytes.to_string()
                } else {
                    String::new()
                }
            };

            if !delta.is_empty() {
                let event = TaskOutputEvent {
                    task_id: *id,
                    session_id: *session_id as i64,
                    pid: *pid,
                    command: command.clone(),
                    event_type: "output".to_string(),
                    output_delta: delta,
                    out_file: out_file.clone(),
                    start_time: *start_time,
                    exit_code: None,
                };
                let _ = app.emit("background_task_output", &event);
            }

            // Check if process is still alive
            let alive = is_process_alive(*pid);
            if !alive {
                // Last read + mark done
                let bytes = std::fs::read(out_file).unwrap_or_default();
                let content = String::from_utf8_lossy(&bytes).to_string();
                let total = content.len() as u64;
                let mut reg = registry().lock().unwrap();
                let cursor = reg.cursors.get(id).copied().unwrap_or(0);
                let final_delta = if total > cursor {
                    content[cursor as usize..].to_string()
                } else {
                    String::new()
                };
                reg.cursors.insert(*id, total);
                if let Some(t) = reg.tasks.get_mut(id) {
                    t.running = false;
                }
                drop(reg);

                let event = TaskOutputEvent {
                    task_id: *id,
                    session_id: *session_id as i64,
                    pid: *pid,
                    command: command.clone(),
                    event_type: "exited".to_string(),
                    output_delta: final_delta,
                    out_file: out_file.clone(),
                    start_time: *start_time,
                    exit_code: None,
                };
                let _ = app.emit("background_task_output", &event);
            }
        }
    });
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        crate::agent_service::hidden_cmd("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        crate::agent_service::hidden_cmd("ps")
            .args(["-p", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: register a fake task for testing.
    fn register_fake(session_id: i64, pid: u32, cmd: &str) -> u64 {
        let tmp = std::env::temp_dir();
        let out = tmp.join("test_bg_out.txt");
        let err = tmp.join("test_bg_err.txt");
        // Clean up any leftover files
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&err);
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
        let tmp = std::env::temp_dir().join("test_bg_read.txt");
        std::fs::write(&tmp, "line1\nline2\nline3\nline4\nline5").unwrap();
        let result = read_output(&tmp.to_string_lossy(), 3);
        assert_eq!(result.lines().count(), 3);
        assert!(result.contains("line3"));
        assert!(result.contains("line5"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn read_output_tail_more_than_content() {
        let tmp = std::env::temp_dir().join("test_bg_read2.txt");
        std::fs::write(&tmp, "only\n").unwrap();
        let result = read_output(&tmp.to_string_lossy(), 100);
        assert_eq!(result, "only");
        let _ = std::fs::remove_file(&tmp);
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
    fn task_id_increments() {
        let id1 = register_fake(40, 4001, "test");
        let id2 = register_fake(40, 4002, "test");
        assert!(id2 > id1);
    }

    #[test]
    fn task_has_out_file_and_err_file() {
        let id = register_fake(50, 5001, "cmd");
        let tasks = list_tasks(50);
        let t = tasks.iter().find(|t| t.id == id).unwrap();
        assert!(!t.out_file.is_empty());
        assert!(!t.err_file.is_empty());
        assert!(t.out_file.contains("test_bg_out"));
        assert!(t.err_file.contains("test_bg_err"));
    }
}
