// Runner — per-session concurrency control.
//
// Ensures only one agent loop runs per session at a time.
// If a new prompt arrives while the session is busy, the old stream is aborted
// and the new one starts after a brief grace period.

use std::collections::HashMap;
use std::sync::Mutex;

/// State of a session's runner.
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerState {
    /// No work in progress.
    Idle,
    /// An agent loop is running.
    Running,
}

struct SessionRunner {
    state: RunnerState,
}

pub struct RunnerRegistry {
    sessions: Mutex<HashMap<i64, SessionRunner>>,
}

impl RunnerRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Try to acquire the runner for a session. Returns true if acquired (session was idle).
    /// Returns false if the session is already running.
    pub fn try_acquire(&self, session_id: i64) -> bool {
        let mut map = self.sessions.lock().unwrap();
        let runner = map.entry(session_id).or_insert(SessionRunner {
            state: RunnerState::Idle,
        });
        if runner.state == RunnerState::Idle {
            runner.state = RunnerState::Running;
            true
        } else {
            false
        }
    }

    /// Release the runner for a session (mark as idle).
    pub fn release(&self, session_id: i64) {
        let mut map = self.sessions.lock().unwrap();
        if let Some(runner) = map.get_mut(&session_id) {
            runner.state = RunnerState::Idle;
        }
    }

    /// Check if a session is currently running.
    #[allow(dead_code)]
    pub fn is_running(&self, session_id: i64) -> bool {
        let map = self.sessions.lock().unwrap();
        map.get(&session_id)
            .map(|r| r.state == RunnerState::Running)
            .unwrap_or(false)
    }
}
