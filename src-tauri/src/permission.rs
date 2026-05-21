// Permission Engine — three-mode trust system + command safety classification
//
// Full:    No confirmations. Everything auto-approved.
// Normal:  Safe commands auto, dangerous ask. Files inside project auto, outside ask.
// Guarded: Everything except reads asks for confirmation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PermissionMode {
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "guarded")]
    Guarded,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    pub tool: String,
    pub file: Option<String>,
    pub command: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PermissionResponse {
    pub id: String,
    pub action: PermissionAction,
    pub always: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    Deny,
}

// ---- Command Safety ----

const SAFE_COMMANDS: &[&str] = &[
    // Navigation
    "ls", "dir", "pwd", "cd", "tree",
    // Read-only
    "cat", "head", "tail", "less", "wc",
    "grep", "rg", "find", "locate", "which", "where", "type",
    "echo", "date", "env", "printenv", "uname", "whoami", "hostname",
    // Git read
    "git status", "git log", "git diff", "git branch", "git show", "git remote",
    "git stash list", "git tag", "git describe", "git rev-parse",
    // Build/test (read/verify)
    "npm test", "npm run build", "npm run dev", "npm run lint",
    "npm run format", "npm run check", "npm run typecheck",
    "npx tsc", "npx tsc --noEmit", "npx eslint", "npx prettier --check",
    "cargo check", "cargo test", "cargo build", "cargo clippy",
    "cargo fmt --check", "cargo doc",
    "go build", "go test", "go vet", "go fmt",
    "python -m pytest", "python -m ruff check", "python -m mypy",
    "python -m black --check", "pip list",
    "make", "make build", "make test",
    // Process
    "ps", "top", "htop", "df", "du",
];

const DANGEROUS_ARGS: &[&str] = &[
    // Destructive
    "-rf", "-fr", "--recursive", "--force",
    // Delete
    "-D", "--delete", "--delete-branch", "branch -d", "branch -D",
    // Force push
    "--force", "-f",
    // Git destructive
    "reset --hard", "clean -f", "rebase --abort",
    // RCE vectors
    "-exec", "-execdir", "-ok",
    // Mutating fixes
    "--fix", "--write", "-w",
    // System-level
    "sudo", "chmod", "chown",
];

const SENSITIVE_PATTERNS: &[&str] = &[
    "*.env", "*.env.*", ".env.*", ".envrc",
    "*.key", "*.pem", "*.p12", "*.pfx",
    "id_rsa*", "id_ed25519*", "id_ecdsa*",
    "*credentials*", "*credential*", "*secret*", "*token*",
    "*password*", "*passwd*",
    "~/.ssh", "~/.aws", "~/.gcloud", "~/.azure",
    "~/.kube", "~/.docker", "~/.github",
    "/etc/shadow", "/etc/sudoers", "/etc/passwd",
    "*.git/config", "*.git/credentials",
];

// ---- Service ----

pub struct PermissionService {
    pub mode: PermissionMode,
    /// Tool -> patterns that are always allowed this session
    session_allow: HashMap<String, Vec<String>>,
    /// Pending confirmations
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<PermissionAction>>>>,
}

impl PermissionService {
    pub fn new() -> Self {
        Self {
            mode: PermissionMode::default(),
            session_allow: HashMap::new(),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Register a pending confirmation and return a receiver to await.
    pub fn register_pending(&self) -> (String, oneshot::Receiver<PermissionAction>) {
        let id = uuid_v4();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);
        (id, rx)
    }

    /// Resolve a pending permission and optionally record "always allow".
    pub fn resolve_pending(&mut self, id: &str, tool: &str, action: PermissionAction, always: bool) -> bool {
        let result = {
            let mut pending = self.pending.lock().unwrap();
            if let Some(tx) = pending.remove(id) {
                let _ = tx.send(action);
                true
            } else {
                false
            }
        };
        if result && always && action == PermissionAction::Allow {
            self.record_allow(tool, "*");
        }
        result
    }

    /// Clear all pending (on cancel/session end).
    #[allow(dead_code)]
    pub fn cancel_all_pending(&self) {
        let _ = self.pending.lock().unwrap().drain().map(|(k, _)| k).collect::<Vec<_>>();
        // All senders dropped → receivers get Err(RecvError)
    }

    /// Record an "always allow" decision for this session.
    pub fn record_allow(&mut self, tool: &str, pattern: &str) {
        self.session_allow
            .entry(tool.to_string())
            .or_default()
            .push(pattern.to_string());
    }

    /// Check if a tool was already always-allowed this session.
    pub fn is_session_allowed(&self, tool: &str, pattern: &str) -> bool {
        self.session_allow
            .get(tool)
            .map(|ps| ps.iter().any(|p| wildcard_match(p, pattern)))
            .unwrap_or(false)
    }

    // ---- Tool Gating ----

    /// Returns Ok(()) if allowed, Err(reason) if should be denied.
    /// Returns None if confirmation is needed.
    pub fn evaluate(
        &self,
        tool: &str,
        file_path: Option<&str>,
        command: Option<&str>,
    ) -> Option<Result<(), String>> {
        // === Always-deny: override all modes, never ask, just block ===
        if let Some(reason) = always_denied(tool, file_path, command) {
            return Some(Err(reason));
        }

        match self.mode {
            PermissionMode::Full => {
                // Everything allowed silently — always_denied handles the truly dangerous
                // (rm -rf, force-push main, system paths)
                Some(Ok(()))
            }
            PermissionMode::Normal => {
                // Read-only tools: always allow
                if is_read_only_tool(tool) {
                    return Some(Ok(()));
                }

                // Session-already-allowed: check by file path and by tool name
                if self.is_session_allowed(tool, "*") {
                    return Some(Ok(()));
                }
                if let Some(fp) = file_path {
                    if self.is_session_allowed(tool, fp) {
                        return Some(Ok(()));
                    }
                }

                // Write tools on sensitive paths: always block
                if let Some(fp) = file_path {
                    if is_sensitive_path(fp) && is_write_tool(tool) {
                        return Some(Err(format!("Blocked: {} matches sensitive path pattern", fp)));
                    }
                }

                // Dangerous command args: always ask
                if let Some(cmd) = command {
                    if self.has_dangerous_args(cmd) {
                        return None;
                    }
                }

                // Safe commands: auto-allow
                if let Some(cmd) = command {
                    if is_safe_command(cmd) {
                        return Some(Ok(()));
                    }
                    // Unknown command → ask
                    return None;
                }

                // git_commit, delete_file, run_command (no command parsed) → ask
                if matches!(tool, "git_commit" | "delete_file" | "run_command" | "run_background" | "run_tests") {
                    return None;
                }

                // File edits: allow inside project, ask outside
                if is_write_tool(tool) {
                    if let Some(fp) = file_path {
                        if is_outside_project(fp) {
                            return None;
                        }
                    }
                    return Some(Ok(()));
                }

                // Default for Normal: allow
                Some(Ok(()))
            }
            PermissionMode::Guarded => {
                // Read-only: always allow
                if is_read_only_tool(tool) {
                    return Some(Ok(()));
                }

                // Session-already-allowed: check by tool name and file path
                if self.is_session_allowed(tool, "*") {
                    return Some(Ok(()));
                }
                if let Some(fp) = file_path {
                    if self.is_session_allowed(tool, fp) {
                        return Some(Ok(()));
                    }
                }

                // Sensitive paths: always block
                if let Some(fp) = file_path {
                    if is_sensitive_path(fp) && is_write_tool(tool) {
                        return Some(Err(format!("Blocked: {} matches sensitive path pattern", fp)));
                    }
                }

                // Everything else → ask
                None
            }
        }
    }

    fn has_dangerous_args(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase();
        DANGEROUS_ARGS.iter().any(|arg| cmd_lower.contains(arg))
    }
}

// ---- Helpers ----

fn always_denied(tool: &str, file_path: Option<&str>, command: Option<&str>) -> Option<String> {
    // Destructive recursive removal
    if let Some(cmd) = command {
        let c = cmd.to_lowercase();
        if (c.contains("rm ") || c.contains("rm\t")) && (c.contains("-rf") || c.contains("-fr") || c.contains("--recursive")) {
            return Some("rm -rf is always blocked. Delete individual files instead.".to_string());
        }
        if c.contains("git push") && (c.contains("--force") || c.contains(" -f")) && (c.contains("main") || c.contains("master")) {
            return Some("Force push to main/master is always blocked.".to_string());
        }
    }

    // File ops on system directories
    if let Some(fp) = file_path {
        let normalized = fp.replace('\\', "/");
        if is_write_tool(tool) {
            for prefix in &["/etc/", "/usr/", "/bin/", "/boot/", "/dev/", "/proc/", "/sys/", "C:/Windows/", "C:/Program Files/", "C:/ProgramData/"] {
                if normalized.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    return Some(format!("File operation on system path '{}' is blocked.", fp));
                }
            }
        }
    }

    None
}

fn is_read_only_tool(tool: &str) -> bool {
    matches!(
        tool,
        "read_file" | "read_files" | "list_dir" | "directory_tree" | "get_file_info"
            | "search_in_dir" | "search_files" | "glob"
            | "git_status" | "git_log" | "git_diff" | "git_branch"
            | "code_graph_search" | "code_graph_callers" | "code_graph_callees"
            | "code_graph_explore" | "code_graph_file" | "code_graph_stats"
            | "web_search" | "web_fetch" | "understand_image"
            | "read_knowledge" | "list_skills" | "match_skills"
            | "job_output" | "list_jobs" | "get_env_info"
            | "analyze_project_structure" | "read_lints"
            // Communication (no file mutation)
            | "send_to_agent" | "ask_choice"
    )
}

fn is_write_tool(tool: &str) -> bool {
    !is_read_only_tool(tool)
}

fn is_sensitive_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    SENSITIVE_PATTERNS.iter().any(|&p| {
        let pattern = p.replace('\\', "/").to_lowercase();
        // Simple wildcard match
        if pattern.contains('*') {
            let re_pattern = pattern
                .replace('.', r"\.")
                .replace('*', ".*")
                .replace('?', ".");
            regex::Regex::new(&format!("(^|/){}(/|$)", re_pattern))
                .map(|re| re.is_match(&normalized))
                .unwrap_or(false)
        } else {
            normalized.contains(&pattern)
        }
    })
}

fn is_safe_command(cmd: &str) -> bool {
    let trimmed = cmd.trim().to_lowercase();
    SAFE_COMMANDS.iter().any(|&safe| trimmed.starts_with(safe))
}

fn is_outside_project(path: &str) -> bool {
    // Simple heuristic: absolute paths outside current dir
    if cfg!(windows) {
        path.contains(":\\Users\\") && !path.contains(":\\project")
    } else {
        path.starts_with("/home/") || path.starts_with("/Users/") || path.starts_with("/etc/")
            || path.starts_with("/usr/") || path.starts_with("/var/")
    }
}

fn wildcard_match(pattern: &str, target: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == target;
    }
    let re_pattern = pattern
        .replace('.', r"\.")
        .replace('*', ".*")
        .replace('?', ".");
    regex::Regex::new(&format!("^{}$", re_pattern))
        .map(|re| re.is_match(target))
        .unwrap_or(false)
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("perm_{:x}", ts)
}
