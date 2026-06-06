// LSP Client — JSON-RPC 2.0 over stdio for Language Server Protocol
//
// - Spawns an LSP server process, initializes the connection
// - Tracks opened files with version numbers (didOpen vs didChange)
// - Detects process crashes and reconnects automatically
// - Supports npx fallback for npm-based servers

use crate::lsp_types::*;
use crate::LockMap;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn hidden_cmd(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = std::process::Command::new(program.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const INIT_TIMEOUT_MS: u64 = 30_000;
const DIAGNOSTICS_DEBOUNCE_MS: u64 = 150;

pub struct LspClient {
    pub server_id: String,
    pub root: String,
    writer: Arc<Mutex<BufWriter<ChildStdin>>>,
    pub diagnostics: Arc<Mutex<HashMap<String, Vec<LspDiagnostic>>>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<Value>>>>,
    process: Mutex<Child>,
    pid: u32,
    // File version tracking: path -> version (0 = opened once, increments on didChange)
    file_versions: Arc<Mutex<HashMap<String, u32>>>,
    // Process liveness (reader thread sets to false on exit)
    alive: Arc<AtomicBool>,
    // Stored for reconnect
    spawn_cmd: String,
    spawn_args: Vec<String>,
}

impl LspClient {
    /// Spawn an LSP server process. Returns Err if the server cannot be started.
    pub fn spawn(
        server_id: &str,
        root: &str,
        cmd: &str,
        args: &[String],
    ) -> Result<Self, String> {
        let mut child = hidden_cmd(cmd)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("[lsp] Failed to spawn {}: {}", server_id, e))?;

        let pid = child.id();
        crate::process_manager::register_process(
            pid,
            crate::process_manager::ProcessType::Lsp,
            &format!("{} {}", cmd, args.join(" ")),
            None,
            None,
            None,
        );

        let stdin = child.stdin.take().ok_or_else(|| format!("[lsp] {}: No stdin", server_id))?;
        let stdout = child.stdout.take().ok_or_else(|| format!("[lsp] {}: No stdout", server_id))?;
        let writer = Arc::new(Mutex::new(BufWriter::new(stdin)));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<LspDiagnostic>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let file_versions: Arc<Mutex<HashMap<String, u32>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));

        let reader_diags = diagnostics.clone();
        let reader_pending = pending.clone();
        let reader_alive = alive.clone();
        let reader_sid = server_id.to_string();
        std::thread::spawn(move || {
            read_loop(stdout, &reader_diags, &reader_pending, &reader_alive, &reader_sid);
        });

        Ok(Self {
            server_id: server_id.to_string(),
            root: root.to_string(),
            writer,
            diagnostics,
            next_id: AtomicU64::new(1),
            pending,
            process: Mutex::new(child),
            pid,
            file_versions,
            alive,
            spawn_cmd: cmd.to_string(),
            spawn_args: args.to_vec(),
        })
    }

    /// Try spawning with direct command, then npx fallback for npm-based servers.
    pub fn spawn_with_fallback(
        server_id: &str,
        root: &str,
        cmd: &str,
        args: &[String],
    ) -> Result<Self, String> {
        match Self::spawn(server_id, root, cmd, args) {
            Ok(client) => Ok(client),
            Err(e1) => {
                if is_npm_server(cmd) {
                    eprintln!("[lsp] {} not found, trying npx {}...", server_id, cmd);
                    let mut npx_args = vec!["-y".to_string(), cmd.to_string()];
                    npx_args.extend(args.iter().cloned());
                    Self::spawn(server_id, root, "npx", &npx_args)
                        .map_err(|e2| format!("{} | npx fallback: {}", e1, e2))
                } else {
                    Err(e1)
                }
            }
        }
    }

    /// Check if the process is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Reconnect after process crash. Kills old process, spawns new one,
    /// re-initializes, and re-opens all tracked files.
    pub fn reconnect(&mut self) -> Result<(), String> {
        eprintln!("[lsp:{}] Reconnecting...", self.server_id);

        // Kill old process and mark exited
        crate::process_manager::mark_exited(self.pid, None);
        let _ = self.process.lock_str()?.kill();
        self.alive.store(false, Ordering::SeqCst);

        // Spawn new process
        let mut child = hidden_cmd(&self.spawn_cmd)
            .args(&self.spawn_args)
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Re-spawn failed: {}", e))?;

        self.pid = child.id();
        crate::process_manager::register_process(
            self.pid,
            crate::process_manager::ProcessType::Lsp,
            &format!("{} {}", self.spawn_cmd, self.spawn_args.join(" ")),
            None,
            None,
            None,
        );

        let stdin = child.stdin.take().ok_or("No stdin")?;
        let stdout = child.stdout.take().ok_or("No stdout")?;
        *self.writer.lock_str()? = BufWriter::new(stdin);

        self.alive.store(true, Ordering::SeqCst);
        let reader_diags = self.diagnostics.clone();
        let reader_pending = self.pending.clone();
        let reader_alive = self.alive.clone();
        let reader_sid = self.server_id.clone();
        std::thread::spawn(move || {
            read_loop(stdout, &reader_diags, &reader_pending, &reader_alive, &reader_sid);
        });

        *self.process.lock_str()? = child;

        // Re-initialize
        self.initialize()?;

        // Re-open all tracked files
        let files: Vec<(String, String, String)> = {
            let versions = self.file_versions.lock_str()?;
            versions.keys().map(|path| {
                let content = std::fs::read_to_string(path).unwrap_or_default();
                let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
                let lang_id = ext_to_lang_id(ext);
                (path.clone(), content, lang_id.to_string())
            }).collect()
        };
        for (path, content, lang_id) in &files {
            let uri = path_to_uri(path);
            self.notify("textDocument/didOpen", serde_json::json!({
                "textDocument": { "uri": uri, "languageId": lang_id, "version": 0, "text": content }
            }));
        }
        // Reset version tracking to 0 after reconnect
        {
            let mut versions = self.file_versions.lock_str()?;
            let paths: Vec<String> = versions.keys().cloned().collect();
            for path in paths {
                *versions.get_mut(&path).ok_or_else(|| format!("path not tracked: {}", path))? = 0;
            }
        }

        eprintln!("[lsp:{}] Reconnected with {} files", self.server_id, files.len());
        Ok(())
    }

    /// Send the `initialize` request and `initialized` notification.
    pub fn initialize(&self) -> Result<(), String> {
        let root_uri = path_to_uri(&self.root);
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [{
                "name": "workspace",
                "uri": root_uri
            }],
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "versionSupport": true }
                },
                "workspace": {
                    "didChangeWatchedFiles": { "dynamicRegistration": true }
                }
            }
        });

        self.request("initialize", init_params, INIT_TIMEOUT_MS)
            .map_err(|e| format!("Initialize failed: {}", e))?;

        self.notify("initialized", serde_json::json!({}));
        eprintln!("[lsp:{}] initialized", self.server_id);
        Ok(())
    }

    /// Touch a file: sends didOpen (first time) or didChange (subsequent).
    /// Also sends workspace/didChangeWatchedFiles.
    pub fn touch_file(&self, path: &str, content: &str, language_id: &str) -> Result<(), String> {
        let uri = path_to_uri(path);
        let mut versions = self.file_versions.lock_str()?;

        if let Some(version) = versions.get_mut(path) {
            // File already open — send didChange
            *version += 1;
            self.notify("workspace/didChangeWatchedFiles", serde_json::json!({
                "changes": [{
                    "uri": uri,
                    "type": 2 // Changed
                }]
            }));
            self.notify("textDocument/didChange", serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "version": *version
                },
                "contentChanges": [{ "text": content }]
            }));
        } else {
            // First time — send didOpen
            versions.insert(path.to_string(), 0);
            self.notify("workspace/didChangeWatchedFiles", serde_json::json!({
                "changes": [{
                    "uri": uri,
                    "type": 1 // Created
                }]
            }));
            // Clear stale diagnostics for this file
            let _ = self.diagnostics.lock_str().map(|mut d| d.remove(path));
            self.notify("textDocument/didOpen", serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 0,
                    "text": content
                }
            }));
        }
        Ok(())
    }

    /// Wait for diagnostics to arrive for the given file.
    pub fn wait_for_diagnostics(&self, path: &str, timeout_ms: u64) -> Vec<LspDiagnostic> {
        let start = std::time::Instant::now();
        std::thread::sleep(Duration::from_millis(200));

        loop {
            let diags = self.get_diagnostics(path);
            if !diags.is_empty() || start.elapsed().as_millis() as u64 > timeout_ms {
                return diags;
            }
            std::thread::sleep(Duration::from_millis(DIAGNOSTICS_DEBOUNCE_MS));
        }
    }

    /// Get cached diagnostics for a file.
    pub fn get_diagnostics(&self, path: &str) -> Vec<LspDiagnostic> {
        self.diagnostics.lock_str()
            .ok()
            .and_then(|d| d.get(path).cloned())
            .unwrap_or_default()
    }

    /// Get all cached diagnostics.
    pub fn all_diagnostics(&self) -> HashMap<String, Vec<LspDiagnostic>> {
        self.diagnostics.lock_str()
            .map(|d| d.clone())
            .unwrap_or_default()
    }

    /// Shut down the client and kill the process.
    pub fn shutdown(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.request("shutdown", serde_json::json!({}), 3000);
        self.notify("exit", serde_json::json!({}));
        if let Ok(mut p) = self.process.lock_str() {
            let _ = p.kill();
        }
        crate::process_manager::mark_exited(self.pid, None);
    }

    // ---- Internal ----

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn request(&self, method: &str, params: Value, timeout_ms: u64) -> Result<Value, String> {
        let id = self.next_request_id();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            self.pending.lock_str()?.insert(id, tx);
        }
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;

        rx.recv_timeout(Duration::from_millis(timeout_ms))
            .map_err(|e| format!("Request '{}' timed out: {}", method, e))
            .and_then(|v| {
                if let Some(err) = v.get("error") {
                    Err(format!("LSP error: {}", err))
                } else {
                    Ok(v.get("result").cloned().unwrap_or(Value::Null))
                }
            })
    }

    fn notify(&self, method: &str, params: Value) {
        let _ = self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }));
    }

    fn write_message(&self, msg: &Value) -> Result<(), String> {
        let body = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut writer = self.writer.lock_str()?;
        writer.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
        writer.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        crate::process_manager::mark_exited(self.pid, None);
        if let Ok(mut p) = self.process.lock_str() {
            let _ = p.kill();
        }
    }
}

// ---- Reader Thread ----

fn read_loop(
    stdout: ChildStdout,
    diagnostics: &Arc<Mutex<HashMap<String, Vec<LspDiagnostic>>>>,
    pending: &Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<Value>>>>,
    alive: &Arc<AtomicBool>,
    server_id: &str,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let msg = match read_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(e) => {
                eprintln!("[lsp:{}] Read error: {}", server_id, e);
                break;
            }
        };

        if msg.get("method").is_some() {
            let method = msg["method"].as_str().unwrap_or("");
            if method == "textDocument/publishDiagnostics" {
                if let Some(params) = msg.get("params") {
                    if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone()) {
                        let path = uri_to_path(&p.uri);
                        eprintln!("[lsp:{}] diagnostics: {} ({} issues)", server_id, path, p.diagnostics.len());
                        if let Ok(mut d) = diagnostics.lock() {
                            d.insert(path, p.diagnostics);
                        }
                    }
                }
            }
        } else if let Some(id) = msg.get("id").and_then(|i| i.as_u64()) {
            if let Ok(mut p) = pending.lock() {
                if let Some(tx) = p.remove(&id) {
                    let _ = tx.send(msg);
                }
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
    eprintln!("[lsp:{}] Reader thread exited", server_id);
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Option<Value>, String> {
    let mut header = String::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(None),
            Ok(_) => {
                header.push_str(&line);
                if line == "\r\n" || line == "\n" {
                    break;
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    let content_length = header
        .lines()
        .find(|l| l.to_lowercase().starts_with("content-length"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .ok_or("Missing Content-Length header")?;

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|e| format!("Failed to read body: {}", e))?;

    let msg: Value = serde_json::from_slice(&body).map_err(|e| format!("JSON parse: {}", e))?;
    Ok(Some(msg))
}

// ---- Helpers ----

fn path_to_uri(path: &str) -> String {
    let abs = std::path::Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    format!(
        "file:///{}",
        abs.to_string_lossy().replace('\\', "/").trim_start_matches('/')
    )
}

fn uri_to_path(uri: &str) -> String {
    let stripped = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);
    let decoded = percent_decode(stripped);
    if cfg!(windows) {
        decoded.replace('/', "\\")
    } else {
        decoded
    }
}

fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_npm_server(cmd: &str) -> bool {
    matches!(
        cmd,
        "typescript-language-server"
            | "vue-language-server"
            | "svelte-language-server"
            | "pyright-langserver"
            | "bash-language-server"
            | "docker-langserver"
            | "yaml-language-server"
    )
}

fn ext_to_lang_id(ext: &str) -> &'static str {
    match ext {
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "mjs" => "javascript",
        "cjs" => "javascript",
        "rs" => "rust",
        "py" | "pyi" => "python",
        "go" => "go",
        "vue" => "vue",
        "svelte" => "svelte",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "dart" => "dart",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "css" => "css",
        "html" | "htm" => "html",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shellscript",
        "dockerfile" | "Dockerfile" => "dockerfile",
        _ => "plaintext",
    }
}
