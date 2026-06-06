// Unified Process Manager — single source of truth for ALL child processes.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Process type classification.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ProcessType {
    /// Agent tool command (run_command)
    Command,
    /// Background task (run_background)
    Background,
    /// LSP server
    Lsp,
    /// MCP server
    Mcp,
}

/// A single tracked process.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedProcess {
    pub pid: u32,
    pub process_type: ProcessType,
    pub command: String,
    pub session_id: Option<i64>,
    pub call_id: Option<String>,
    pub task_id: Option<u64>,
    pub start_time: u64,
    pub running: bool,
    pub exit_code: Option<i32>,
}

struct ProcessRegistry {
    processes: HashMap<u32, ManagedProcess>,
}

static REGISTRY: OnceLock<Arc<Mutex<ProcessRegistry>>> = OnceLock::new();

fn registry() -> &'static Arc<Mutex<ProcessRegistry>> {
    REGISTRY.get_or_init(|| {
        Arc::new(Mutex::new(ProcessRegistry {
            processes: HashMap::new(),
        }))
    })
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Register a new child process.
pub fn register_process(
    pid: u32,
    process_type: ProcessType,
    command: &str,
    session_id: Option<i64>,
    call_id: Option<String>,
    task_id: Option<u64>,
) {
    let proc = ManagedProcess {
        pid,
        process_type,
        command: command.to_string(),
        session_id,
        call_id,
        task_id,
        start_time: now_secs(),
        running: true,
        exit_code: None,
    };
    if let Ok(mut reg) = registry().lock() {
        reg.processes.insert(pid, proc);
    }
}

/// Mark a process as exited.
pub fn mark_exited(pid: u32, exit_code: Option<i32>) {
    if let Ok(mut reg) = registry().lock() {
        if let Some(p) = reg.processes.get_mut(&pid) {
            p.running = false;
            p.exit_code = exit_code;
        }
    }
}

/// Remove a process from tracking entirely.
#[allow(dead_code)]
pub fn unregister(pid: u32) {
    if let Ok(mut reg) = registry().lock() {
        reg.processes.remove(&pid);
    }
}

/// List all tracked processes.
pub fn list_all() -> Vec<ManagedProcess> {
    let reg = registry().lock().unwrap();
    reg.processes.values().cloned().collect()
}

/// List processes for a session.
#[allow(dead_code)]
pub fn list_for_session(session_id: i64) -> Vec<ManagedProcess> {
    let reg = registry().lock().unwrap();
    reg.processes
        .values()
        .filter(|p| p.session_id == Some(session_id))
        .cloned()
        .collect()
}

/// Count running processes.
#[allow(dead_code)]
pub fn running_count() -> usize {
    let reg = registry().lock().unwrap();
    reg.processes.values().filter(|p| p.running).count()
}

/// Get all running PIDs for a session (for abort_stream bulk-kill).
pub fn pids_for_session(session_id: i64) -> Vec<u32> {
    let reg = registry().lock().unwrap();
    reg.processes
        .values()
        .filter(|p| p.running && p.session_id == Some(session_id))
        .map(|p| p.pid)
        .collect()
}

/// Kill all running processes for a session and mark them exited.
pub fn kill_session(session_id: i64) {
    let pids = pids_for_session(session_id);
    for pid in pids {
        eprintln!("[ProcessManager] Killing PID {} for session {}", pid, session_id);
        let _ = kill_tree(pid);
    }
}

/// RAII guard that marks a process as exited on drop.
#[allow(dead_code)]
pub struct ExitGuard {
    pid: u32,
}

impl ExitGuard {
    #[allow(dead_code)]
    pub fn new(pid: u32) -> Self {
        Self { pid }
    }
}

impl Drop for ExitGuard {
    fn drop(&mut self) {
        mark_exited(self.pid, None);
    }
}

/// Kill a process tree (process + all children) on Windows.
/// Graceful first (SIGTERM / taskkill /T), then force after 3s.
pub fn kill_tree(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // Graceful: send WM_CLOSE to the process tree
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        // Wait up to 3s for graceful exit
        std::thread::sleep(std::time::Duration::from_secs(3));
        // Force kill if still alive
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("not found") {
                return Err(format!("taskkill failed: {}", stderr));
            }
        }
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
    mark_exited(pid, Some(-1));
    Ok(())
}

/// Kill all running processes (for app shutdown cleanup).
pub fn kill_all() {
    let pids: Vec<u32> = {
        let reg = registry().lock().unwrap();
        reg.processes
            .values()
            .filter(|p| p.running)
            .map(|p| p.pid)
            .collect()
    };
    for pid in pids {
        eprintln!("[ProcessManager] Killing PID {} on shutdown", pid);
        let _ = kill_tree(pid);
    }
}

/// Cleanup exited processes older than max_age_secs.
pub fn cleanup_exited(max_age_secs: u64) {
    let now = now_secs();
    if let Ok(mut reg) = registry().lock() {
        reg.processes.retain(|_, p| {
            p.running || (now.saturating_sub(p.start_time) < max_age_secs)
        });
    }
}

/// Get memory usage of a process on Windows (returns RSS in bytes, 0 if unavailable).
pub fn get_memory_usage(pid: u32) -> u64 {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-Process -Id {} -ErrorAction SilentlyContinue).WorkingSet64",
                    pid
                ),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                s.parse::<u64>().unwrap_or(0)
            }
            Err(_) => 0,
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Get total memory used by all tracked processes.
pub fn total_memory_bytes() -> u64 {
    let pids: Vec<u32> = {
        let reg = registry().lock().unwrap();
        reg.processes
            .values()
            .filter(|p| p.running)
            .map(|p| p.pid)
            .collect()
    };
    pids.iter().map(|pid| get_memory_usage(*pid)).sum()
}
