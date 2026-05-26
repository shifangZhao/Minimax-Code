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
    pub event_type: String,  // "output" | "exited"
    pub output_delta: String,
    pub exit_code: Option<i32>,
}

struct Registry {
    tasks: HashMap<u64, BackgroundTask>,
    /// Per-task output cursor: byte offset already emitted to frontend.
    cursors: HashMap<u64, u64>,
}

static REGISTRY: OnceLock<Arc<Mutex<Registry>>> = OnceLock::new();
static POLLER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn registry() -> &'static Arc<Mutex<Registry>> {
    REGISTRY.get_or_init(|| {
        Arc::new(Mutex::new(Registry {
            tasks: HashMap::new(),
            cursors: HashMap::new(),
        }))
    })
}

/// Register a newly spawned background process. Returns the task id.
pub(crate) fn register_task(
    session_id: i64,
    pid: u32,
    command: &str,
    out_file: &str,
    err_file: &str,
) -> u64 {
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let task = BackgroundTask {
        id,
        session_id,
        pid,
        command: command.to_string(),
        out_file: out_file.to_string(),
        err_file: err_file.to_string(),
        start_time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        running: true,
        exit_code: None,
    };
    let mut reg = registry().lock().unwrap();
    reg.cursors.insert(id, 0);
    reg.tasks.insert(id, task);
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

/// Read output file and return full content.
pub(crate) fn read_output(out_file: &str, tail_lines: usize) -> String {
    let content = std::fs::read_to_string(out_file).unwrap_or_default();
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
    if POLLER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let snap: Vec<(u64, u64, u32, String, String)> = {
            let reg = registry().lock().unwrap();
            reg.tasks
                .values()
                .filter(|t| t.running)
                .map(|t| (t.id, t.session_id as u64, t.pid, t.out_file.clone(), t.command.clone()))
                .collect()
        };
        for (id, session_id, pid, out_file, command) in &snap {
            let content = std::fs::read_to_string(out_file).unwrap_or_default();
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
                    exit_code: None,
                };
                let _ = app.emit("background_task_output", &event);
            }

            // Check if process is still alive
            let alive = is_process_alive(*pid);
            if !alive {
                // Last read + mark done
                let content = std::fs::read_to_string(out_file).unwrap_or_default();
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
