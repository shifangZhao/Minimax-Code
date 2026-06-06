// Permission Engine — three-mode trust system + command safety classification
//
// Full:    No confirmations. Everything auto-approved.
// Normal:  Safe commands auto, dangerous ask. Files inside project auto, outside ask.
// Guarded: Everything except reads asks for confirmation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub enum PermissionMode {
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "normal")]
    #[default]
    Normal,
    #[serde(rename = "guarded")]
    Guarded,
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
    // Secrets & credentials
    "*.env", "*.env.*", ".env.*", ".envrc",
    "*.key", "*.pem", "*.p12", "*.pfx",
    "id_rsa*", "id_ed25519*", "id_ecdsa*",
    "*credentials*", "*credential*", "*secret*", "*token*",
    "*password*", "*passwd*",
    // Unix home-dir sensitive
    "~/.ssh", "~/.aws", "~/.gcloud", "~/.azure",
    "~/.kube", "~/.docker", "~/.github",
    // Unix system files
    "/etc/shadow", "/etc/sudoers", "/etc/passwd",
    "/etc/group", "/etc/hosts", "/etc/resolv.conf",
    "*.git/config", "*.git/credentials",
    // Windows system directories
    "c:/windows/system32", "c:/windows/syswow64",
    "c:/windows/system", "c:/windows/security",
    "c:/windows/registration", "c:/windows/win.ini",
    "c:/windows/system.ini",
    "c:/program files", "c:/program files (x86)",
    "c:/programdata", "c:/users/all users",
    "c:/boot", "c:/efi",
    // Windows registry & SAM
    "c:/windows/system32/config/sam",
    "c:/windows/system32/config/security",
    "c:/windows/system32/config/system",
    "c:/windows/system32/config/software",
    // Windows credential stores
    "*\\microsoft\\credentials*", "*\\microsoft\\protect*",
    // SSH keys on Windows
    "*/.ssh/id_*",
    // Common sensitive locations
    "/root/", "/var/log/", "/proc/", "/sys/",
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
    pub fn register_pending(&self) -> Result<(String, oneshot::Receiver<PermissionAction>), String> {
        let id = uuid_v4();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().map_err(|e| format!("mutex poisoned: {}", e))?.insert(id.clone(), tx);
        Ok((id, rx))
    }

    /// Resolve a pending permission and optionally record "always allow".
    pub fn resolve_pending(&mut self, id: &str, tool: &str, action: PermissionAction, always: bool) -> bool {
        let result = {
            match self.pending.lock() {
                Ok(mut pending) => {
                    if let Some(tx) = pending.remove(id) {
                        let _ = tx.send(action);
                        true
                    } else {
                        false
                    }
                }
                Err(_) => false,
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
        if let Ok(mut pending) = self.pending.lock() {
            let _ = pending.drain().map(|(k, _)| k).collect::<Vec<_>>();
        }
        // All senders dropped → receivers get Err(RecvError)
    }

    /// Record an "always allow" decision for this session.
    pub fn record_allow(&mut self, tool: &str, pattern: &str) {
        self.session_allow
            .entry(tool.to_string())
            .or_default()
            .push(pattern.to_string());
    }

    /// Load persisted rules from a list of (tool, pattern) pairs.
    pub fn load_rules_batch(&mut self, rules: &[(String, String)]) {
        for (tool, pattern) in rules {
            self.record_allow(tool, pattern);
        }
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

                // Safe commands and command chains: auto-allow
                if let Some(cmd) = command {
                    if let Some(result) = check_command_chain(cmd) {
                        return Some(result);
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
            | "web_search" | "web_fetch" | "understand_image"
            | "read_knowledge" | "list_skills" | "match_skills"
            | "job_output" | "list_jobs" | "get_env_info"
            | "analyze_project_structure" | "read_lints"
            // Communication (no file mutation)
            | "ask_choice"
    )
}

fn is_write_tool(tool: &str) -> bool {
    !is_read_only_tool(tool)
}

fn normalize_path_for_check(path: &str) -> String {
    // Strip NTFS alternate data streams (e.g. file.txt:Zone.Identifier → file.txt)
    let without_stream = match path.find(':') {
        Some(pos) if cfg!(windows) => {
            let before_colon = &path[..pos];
            // Only strip if it looks like an ADS (not a drive letter like C:)
            if before_colon.len() > 2 { before_colon.to_string() } else { path.to_string() }
        }
        _ => path.to_string(),
    };
    // Normalize separators and lowercase
    let s = without_stream.replace('\\', "/").to_lowercase();
    // Collapse "." and ".." components
    let parts: Vec<&str> = s.split('/').collect();
    let mut out: Vec<&str> = Vec::new();
    for p in parts {
        if p == "." || p.is_empty() { continue; }
        if p == ".." { out.pop(); continue; }
        out.push(p);
    }
    out.join("/")
}

/// Pre-compiled sensitive-path regexes. Compiling a regex is expensive
/// (parses, allocates, builds a state machine); SENSITIVE_PATTERNS is static
/// so we should compile each pattern exactly once, not on every check call.
static SENSITIVE_REGEXES: Lazy<Vec<regex::Regex>> = Lazy::new(|| {
    SENSITIVE_PATTERNS
        .iter()
        .filter_map(|&p| {
            let pattern = p.replace('\\', "/").to_lowercase();
            if pattern.contains('*') {
                let re_pattern = pattern
                    .replace('.', r"\.")
                    .replace('*', ".*")
                    .replace('?', ".");
                regex::Regex::new(&format!("(^|/){}(/|$)", re_pattern)).ok()
            } else {
                None
            }
        })
        .collect()
});

/// Pre-computed non-glob patterns as a Vec<String> (lowercased, with
/// forward-slash normalization) for fast substring checks.
static SENSITIVE_LITERALS: Lazy<Vec<String>> = Lazy::new(|| {
    SENSITIVE_PATTERNS
        .iter()
        .filter(|p| !p.contains('*'))
        .map(|p| p.replace('\\', "/").to_lowercase())
        .collect()
});

fn is_sensitive_path(path: &str) -> bool {
    let normalized = normalize_path_for_check(path);
    // Fast path: literal (non-glob) patterns use plain substring/prefix/equality
    if SENSITIVE_LITERALS.iter().any(|p| {
        normalized.contains(&format!("/{}", p))
            || normalized.starts_with(p)
            || normalized == *p
    }) {
        return true;
    }
    // Slow path: glob patterns use pre-compiled regexes.
    SENSITIVE_REGEXES.iter().any(|re| re.is_match(&normalized))
}

fn is_safe_command(cmd: &str) -> bool {
    let trimmed = cmd.trim().to_lowercase();
    // First token (the program name) must exactly match a safe command — never
    // a starts_with() check, which would let "lsof" pass as "ls", "psgrep" as
    // "ps", "findsecret" as "find", etc. Multi-word safe entries like
    // "git status" need the full prefix to match exactly.
    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    if first_token.is_empty() {
        return false;
    }
    SAFE_COMMANDS.iter().any(|&safe| {
        if safe.contains(' ') {
            // Multi-word safe entry: must match the full prefix exactly
            // (i.e. up to its own length), with a word boundary after.
            trimmed == safe || trimmed.starts_with(safe) && {
                let rest = &trimmed[safe.len()..];
                rest.chars().next().map_or(true, |c| c.is_whitespace())
            }
        } else {
            // Single-word: match first token exactly, not just as a prefix.
            first_token == safe
        }
    })
}

/// Parse a command string and check git sub-commands.
/// Returns Some(Ok(())) for safe git operations, Some(Err(...)) for dangerous ones,
/// or None if the command isn't a git command or needs confirmation.
fn check_git_command(cmd: &str) -> Option<Result<(), String>> {
    let trimmed = cmd.trim().to_lowercase();

    // Check if it's a git command
    let git_prefix = ["git ", "git\n", "git\r"];
    if !git_prefix.iter().any(|p| trimmed.starts_with(p)) && !trimmed.starts_with("git ") {
        return None;
    }

    // Parse git sub-command
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    match parts[1] {
        // Read-only git commands — always allow
        "status" | "log" | "diff" | "branch" | "show" | "remote"
        | "stash" | "tag" | "describe" | "rev-parse" | "shortlog"
        | "fetch" | "pull" | "clone" | "init" | "mv" | "rm"
        | "add" | "commit" | "push" | "reset" | "rebase"
        | "checkout" | "switch" | "worktree" | "reflog" | "grep" | "lfs" => {
            // Further check for dangerous patterns within args
            let args = &cmd[parts[0].len() + 1..];
            if args.contains("--force") || args.contains(" -f") || args.contains("--delete") {
                // git push --force to main/master is always blocked
                if args.contains("main") || args.contains("master") {
                    return Some(Err("Force push to main/master is always blocked".to_string()));
                }
                // Other force operations need confirmation
                return None;
            }
            // Safe git commands — auto-allow
            Some(Ok(()))
        }
        _ => None, // Unknown git sub-command — fall through to normal confirmation
    }
}

/// Parse a command chain (split by |, &&, ||, ;) and check each command.
/// Returns None if any command needs confirmation, or the aggregate result.
pub(crate) fn check_command_chain(cmd: &str) -> Option<Result<(), String>> {
    let chain = split_command_chain(cmd);
    if chain.len() == 1 {
        // Single command — delegate to existing logic
        if is_safe_command(cmd) {
            return Some(Ok(()));
        }
        if let Some(result) = check_git_command(cmd) {
            return Some(result);
        }
        return None;
    }

    // Multiple commands in chain — check each
    let mut has_unsafe = false;
    for part in &chain {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Extract the actual command (first token) for checking
        let first_token = trimmed.split_whitespace().next().unwrap_or("");

        // Redirect-only commands are safe
        if first_token == "exit" || first_token == "cd" || first_token.is_empty() {
            continue;
        }

        // Check if this part is safe or needs confirmation
        let part_lower = trimmed.to_lowercase();
        if part_lower.contains("rm -rf") || part_lower.contains("fork bomb") {
            return Some(Err(format!("Dangerous command in chain: {}", trimmed)));
        }
        if is_safe_command(trimmed) {
            continue;
        }
        if let Some(result) = check_git_command(trimmed) {
            if result.is_err() {
                return Some(result); // Propagate dangerous git commands
            }
            continue; // Safe git command
        }
        // Unknown command in chain — needs confirmation
        has_unsafe = true;
    }

    if has_unsafe {
        None // Needs confirmation
    } else {
        Some(Ok(()))
    }
}

/// Split a command string by shell chain operators.
fn split_command_chain(cmd: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    let mut paren_depth = 0;

    let chars: Vec<char> = cmd.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // Handle quotes
        if (c == '"' || c == '\'') && !in_quote {
            in_quote = true;
            quote_char = c;
            current.push(c);
            i += 1;
            continue;
        } else if in_quote {
            if c == quote_char {
                in_quote = false;
            }
            current.push(c);
            i += 1;
            continue;
        }

        // Handle parentheses (for subshells)
        if c == '(' {
            paren_depth += 1;
            current.push(c);
            i += 1;
            continue;
        } else if c == ')' {
            paren_depth -= 1;
            current.push(c);
            i += 1;
            continue;
        }

        // Check for operators (only outside quotes and parens)
        if paren_depth == 0 {
            // Check multi-char operators first
            if i + 1 < chars.len() {
                let two = &cmd[i..i + 2];
                if two == "&&" || two == "||" || two == ">>" {
                    // Split before this operator
                    let trimmed = current.trim();
                    if !trimmed.is_empty() {
                        result.push(trimmed.to_string());
                    }
                    current.clear();
                    i += 2;
                    continue;
                }
            }
            if c == '|' || c == ';' || c == '>' {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    result.push(trimmed.to_string());
                }
                current.clear();
                i += 1;
                continue;
            }
        }

        current.push(c);
        i += 1;
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }

    result
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Mode defaults ---

    #[test]
    fn default_mode_is_normal() {
        let ps = PermissionService::new();
        assert_eq!(ps.mode, PermissionMode::Normal);
    }

    #[test]
    fn set_mode_works() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Full);
        assert_eq!(ps.mode, PermissionMode::Full);
        ps.set_mode(PermissionMode::Guarded);
        assert_eq!(ps.mode, PermissionMode::Guarded);
    }

    // --- Full mode: everything auto-allowed ---

    #[test]
    fn full_mode_allows_write_tool() {
        let ps = PermissionService::new();
        let mut ps_full = ps;
        ps_full.set_mode(PermissionMode::Full);
        let result = ps_full.evaluate("write_file", Some("/tmp/test.txt"), None);
        assert_eq!(result, Some(Ok(())));
    }

    #[test]
    fn full_mode_allows_run_command() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Full);
        let result = ps.evaluate("run_command", None, Some("npm install"));
        assert_eq!(result, Some(Ok(())));
    }

    #[test]
    fn full_mode_blocks_always_denied() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Full);
        // rm -rf / should be always denied
        let result = ps.evaluate("run_command", None, Some("rm -rf /"));
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // --- Normal mode: read-only auto, writes ask ---

    #[test]
    fn normal_mode_allows_read_tools() {
        let ps = PermissionService::new(); // defaults to Normal
        assert_eq!(ps.evaluate("read_file", Some("src/main.rs"), None), Some(Ok(())));
        assert_eq!(ps.evaluate("list_dir", Some("."), None), Some(Ok(())));
        assert_eq!(ps.evaluate("search_in_dir", None, None), Some(Ok(())));
    }

    #[test]
    fn normal_mode_safe_commands_auto_allow() {
        let ps = PermissionService::new();
        assert_eq!(ps.evaluate("run_command", None, Some("git status")), Some(Ok(())));
        assert_eq!(ps.evaluate("run_command", None, Some("cargo check")), Some(Ok(())));
        assert_eq!(ps.evaluate("run_command", None, Some("npm test")), Some(Ok(())));
    }

    #[test]
    fn normal_mode_dangerous_args_ask() {
        let ps = PermissionService::new();
        // --force triggers ask
        let result = ps.evaluate("run_command", None, Some("git push --force"));
        assert_eq!(result, None); // None = needs confirmation
    }

    #[test]
    fn normal_mode_unknown_command_ask() {
        let ps = PermissionService::new();
        let result = ps.evaluate("run_command", None, Some("some-unknown-tool --flag"));
        assert_eq!(result, None);
    }

    #[test]
    fn normal_mode_write_tool_inside_project_allow() {
        let ps = PermissionService::new();
        // Path without "/etc", "/root", "c:/windows" etc is considered inside-project
        let result = ps.evaluate("write_file", Some("src/components/App.vue"), None);
        assert_eq!(result, Some(Ok(())));
    }

    #[test]
    fn normal_mode_write_tool_sensitive_path_block() {
        let ps = PermissionService::new();
        let result = ps.evaluate("write_file", Some("/etc/shadow"), None);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // --- Session allow ---

    #[test]
    fn session_allow_star_matches_all() {
        let mut ps = PermissionService::new();
        ps.record_allow("write_file", "*");
        assert!(ps.is_session_allowed("write_file", "anything"));
        assert!(ps.is_session_allowed("write_file", "src/main.rs"));
    }

    #[test]
    fn session_allow_specific_path() {
        let mut ps = PermissionService::new();
        ps.record_allow("write_file", "src/main.rs");
        assert!(ps.is_session_allowed("write_file", "src/main.rs"));
        assert!(!ps.is_session_allowed("write_file", "src/other.rs"));
    }

    #[test]
    fn session_allow_affects_evaluate() {
        let mut ps = PermissionService::new();
        ps.record_allow("run_command", "*");
        // Now unknown commands should be auto-allowed
        let result = ps.evaluate("run_command", None, Some("custom-tool"));
        assert_eq!(result, Some(Ok(())));
    }

    // --- Guarded mode ---

    #[test]
    fn guarded_mode_allows_read_tools() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Guarded);
        assert_eq!(ps.evaluate("read_file", Some("src/main.rs"), None), Some(Ok(())));
    }

    #[test]
    fn guarded_mode_asks_for_write_tools() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Guarded);
        // write_file in Normal would be auto-allowed inside project,
        // but in Guarded it should ask
        let result = ps.evaluate("write_file", Some("src/main.rs"), None);
        assert_eq!(result, None); // needs confirmation
    }

    #[test]
    fn guarded_mode_sensitive_path_blocked() {
        let mut ps = PermissionService::new();
        ps.set_mode(PermissionMode::Guarded);
        let result = ps.evaluate("delete_file", Some("/etc/passwd"), None);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // --- register / resolve pending ---

    #[test]
    fn register_and_resolve_pending() {
        let mut ps = PermissionService::new();
        let (id, _rx) = ps.register_pending().unwrap();
        assert!(!id.is_empty());

        let resolved = ps.resolve_pending(&id, "test_tool", PermissionAction::Allow, false);
        assert!(resolved);
    }

    #[test]
    fn resolve_nonexistent_pending() {
        let mut ps = PermissionService::new();
        let resolved = ps.resolve_pending("nonexistent", "test", PermissionAction::Allow, false);
        assert!(!resolved);
    }

    #[test]
    fn resolve_with_always_records_session_allow() {
        let mut ps = PermissionService::new();
        let (id, _rx) = ps.register_pending().unwrap();
        let resolved = ps.resolve_pending(&id, "write_file", PermissionAction::Allow, true);
        assert!(resolved);
        assert!(ps.is_session_allowed("write_file", "*"));
    }

    #[test]
    fn resolve_deny_does_not_record_session() {
        let mut ps = PermissionService::new();
        let (id, _rx) = ps.register_pending().unwrap();
        let resolved = ps.resolve_pending(&id, "write_file", PermissionAction::Deny, true);
        assert!(resolved);
        assert!(!ps.is_session_allowed("write_file", "*"));
    }

    // --- Helper functions ---

    #[test]
    fn is_read_only_tool_list() {
        assert!(is_read_only_tool("read_file"));
        assert!(is_read_only_tool("list_dir"));
        assert!(is_read_only_tool("search_files"));
        assert!(is_read_only_tool("glob"));
        assert!(!is_read_only_tool("write_file"));
        assert!(!is_read_only_tool("run_command"));
    }

    #[test]
    fn is_write_tool_list() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("edit_file"));
        assert!(is_write_tool("delete_file"));
        assert!(is_write_tool("multi_edit"));
        assert!(!is_write_tool("read_file"));
        // run_command is write-tool (can modify files), not read_only
        assert!(is_write_tool("run_command"));
    }

    #[test]
    fn is_safe_command_list() {
        assert!(is_safe_command("git status"));
        assert!(is_safe_command("ls"));
        assert!(is_safe_command("npm run build"));
        assert!(is_safe_command("cargo check"));
        assert!(!is_safe_command("git push --force origin main"));
        assert!(!is_safe_command("rm -rf /tmp/build"));
    }

    #[test]
    fn has_dangerous_args_detection() {
        let ps = PermissionService::new();
        assert!(ps.has_dangerous_args("git push --force"));
        assert!(ps.has_dangerous_args("rm -rf build"));
        assert!(ps.has_dangerous_args("git reset --hard"));
        assert!(ps.has_dangerous_args("sudo systemctl restart"));
        assert!(!ps.has_dangerous_args("git log"));
        assert!(!ps.has_dangerous_args("npm test"));
    }

    #[test]
    fn is_sensitive_path_detection() {
        // Patterns with wildcards work (e.g. "*.env", "*/.ssh/id_*")
        assert!(is_sensitive_path("/root/.ssh/id_rsa"));
        assert!(is_sensitive_path("/home/user/.aws/credentials"));
        assert!(!is_sensitive_path("src/main.rs"));
        assert!(!is_sensitive_path("package.json"));
        // Windows sensitive paths
        assert!(is_sensitive_path("c:/windows/system32/config/sam"));
        // .env files
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path("project/.env.production"));
    }

    #[test]
    fn always_denied_blocks_destructive_commands() {
        // rm -rf with sensitive paths
        assert!(always_denied("run_command", Some("/"), Some("rm -rf /")).is_some());
        // Force push to main/master
        assert!(always_denied("run_command", None, Some("git push --force origin main")).is_some());
        assert!(always_denied("run_command", None, Some("git push -f origin master")).is_some());
        // Safe commands not blocked
        assert!(always_denied("run_command", None, Some("git status")).is_none());
    }

    #[test]
    fn is_outside_project_detection() {
        #[cfg(windows)]
        {
            // On Windows, the heuristic is: path contains ":\Users\" and not ":\project"
            assert!(is_outside_project("C:\\Users\\Admin\\Documents\\other.txt"));
            assert!(!is_outside_project("C:\\project\\src\\main.rs"));
        }
        #[cfg(not(windows))]
        {
            assert!(is_outside_project("/etc/passwd"));
            assert!(is_outside_project("/tmp/other/file.txt"));
        }
    }
}
