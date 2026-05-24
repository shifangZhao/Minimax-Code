// MCP Service — Model Context Protocol client
//
// Supports both local (stdio child process) and remote (HTTP) transports.
// Config loaded from ~/.minimaxcode/mcp.json (global) and {workspace}/.minimaxcode/mcp.json (project).
// Tool naming: {server_name}_{tool_name} to avoid collisions across servers.

use crate::LockMap;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn hidden_cmd(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program.as_ref());
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
use tokio::sync::RwLock;

// ========== Config Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpServerConfig {
    #[serde(rename = "local")]
    Local {
        command: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default)]
        timeout: Option<u64>,
    },
    #[serde(rename = "remote")]
    Remote {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default)]
        timeout: Option<u64>,
    },
}

fn default_enabled() -> bool {
    true
}

// ========== Tool Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,         // original tool name (without server prefix)
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

// ========== Transport ==========

enum Transport {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

struct StdioTransport {
    writer: Mutex<BufWriter<ChildStdin>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>>,
    process: Mutex<Child>,
    alive: Arc<AtomicBool>,
    stderr_buf: Arc<Mutex<String>>,
}

struct HttpTransport {
    url: String,
    client: HttpClient,
    headers: HashMap<String, String>,
}

impl Transport {
    async fn send_request(&self, request: &serde_json::Value, timeout_ms: u64) -> Result<serde_json::Value, String> {
        match self {
            Transport::Stdio(t) => t.send_request(request, timeout_ms),
            Transport::Http(t) => t.send_request(request, timeout_ms).await,
        }
    }

    async fn send_notification(&self, notification: &serde_json::Value) -> Result<(), String> {
        match self {
            Transport::Stdio(t) => t.send_notification(notification),
            Transport::Http(t) => t.send_notification(notification).await,
        }
    }

    fn is_alive(&self) -> bool {
        match self {
            Transport::Stdio(t) => t.alive.load(Ordering::SeqCst),
            Transport::Http(_) => true,
        }
    }

    fn stderr_snippet(&self) -> String {
        match self {
            Transport::Stdio(t) => {
                if let Ok(sb) = t.stderr_buf.lock() {
                    let s = sb.trim();
                    if s.is_empty() { String::new() } else { format!("\nstderr: {}", &s[..s.len().min(500)]) }
                } else {
                    String::new()
                }
            }
            Transport::Http(_) => String::new(),
        }
    }
}

// ---- Stdio Transport ----

impl StdioTransport {
    fn spawn(command: &[String], env: &HashMap<String, String>, cwd: &str) -> Result<Self, String> {
        let cmd_name = &command[0];
        let cmd_args: Vec<&str> = command[1..].iter().map(|s| s.as_str()).collect();

        let mut cmd = hidden_cmd(cmd_name);
        cmd.args(&cmd_args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Merge environment
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn()
            .map_err(|e| format!("[mcp] Failed to spawn {}: {}", cmd_name, e))?;

        let stdin = child.stdin.take().ok_or_else(|| format!("[mcp] {}: No stdin", cmd_name))?;
        let stdout = child.stdout.take().ok_or_else(|| format!("[mcp] {}: No stdout", cmd_name))?;
        let stderr = child.stderr.take().ok_or_else(|| format!("[mcp] {}: No stderr", cmd_name))?;
        let writer = Mutex::new(BufWriter::new(stdin));
        let pending: Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let stderr_buf = Arc::new(Mutex::new(String::new()));

        let reader_pending = pending.clone();
        let reader_alive = alive.clone();
        std::thread::spawn(move || {
            stdio_read_loop(stdout, &reader_pending, &reader_alive);
        });

        let stderr_buf_clone = stderr_buf.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            let mut reader = std::io::BufReader::new(stderr);
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(s) = String::from_utf8(buf[..n].to_vec()) {
                            if let Ok(mut sb) = stderr_buf_clone.lock() {
                                if sb.len() < 8192 {
                                    sb.push_str(&s);
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            writer,
            next_id: AtomicU64::new(1),
            pending,
            process: Mutex::new(child),
            alive,
            stderr_buf,
        })
    }

    #[allow(dead_code)]
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn send_request(&self, request: &serde_json::Value, timeout_ms: u64) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut req = request.clone();
        req["id"] = serde_json::json!(id);

        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.lock_str()?.insert(id, tx);
        write_stdio_message(&self.writer, &req)?;

        rx.recv_timeout(std::time::Duration::from_millis(timeout_ms))
            .map_err(|e| format!("MCP request timed out after {}ms: {}", timeout_ms, e))
            .and_then(|v| {
                if let Some(err) = v.get("error") {
                    Err(format!("MCP error: {}", err))
                } else {
                    Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
                }
            })
    }

    fn send_notification(&self, notification: &serde_json::Value) -> Result<(), String> {
        write_stdio_message(&self.writer, notification)
    }

}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut p) = self.process.lock_str() {
            let _ = p.kill();
        }
    }
}

fn write_stdio_message(writer: &Mutex<BufWriter<ChildStdin>>, msg: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    let mut w = writer.lock_str()?;
    w.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    w.write_all(b"\n").map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn stdio_read_loop(
    stdout: ChildStdout,
    pending: &Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>>,
    alive: &Arc<AtomicBool>,
) {
    use std::io::Read;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() { continue; }

                // Auto-detect framing:
                // Content-Length: <N>  → read N bytes of JSON (standard MCP)
                // { ... } or other JSON → parse directly (NDJSON)
                if let Some(cl_val) = trimmed
                    .to_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse::<usize>().ok())
                {
                    // Content-Length framing: consume blank line, then read exact bytes
                    let mut blank = String::new();
                    let _ = reader.read_line(&mut blank);  // skip \r\n after header
                    let mut body = vec![0u8; cl_val];
                    if reader.read_exact(&mut body).is_err() { break; }
                    match serde_json::from_slice::<serde_json::Value>(&body) {
                        Ok(msg) => dispatch_msg(msg, pending),
                        Err(e) => eprintln!("[mcp] JSON parse error (Content-Length): {}", e),
                    }
                } else {
                    // NDJSON: each line is one JSON message
                    match serde_json::from_str::<serde_json::Value>(&trimmed) {
                        Ok(msg) => dispatch_msg(msg, pending),
                        Err(e) => eprintln!("[mcp] JSON parse error (NDJSON): {} — {}", e, trimmed),
                    }
                }
            }
            Err(e) => {
                eprintln!("[mcp] Stdio read error: {}", e);
                break;
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
    eprintln!("[mcp] Stdio reader thread exited");
}

fn dispatch_msg(msg: serde_json::Value, pending: &Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>>) {
    if let Some(id) = msg.get("id").and_then(|i| i.as_u64()) {
        if let Ok(mut p) = pending.lock() {
            if let Some(tx) = p.remove(&id) {
                let _ = tx.send(msg);
            }
        }
    }
}

// ---- HTTP Transport ----

impl HttpTransport {
    fn new(url: &str, headers: &HashMap<String, String>, timeout_ms: u64) -> Self {
        let client = HttpClient::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
        Self {
            url: url.to_string(),
            client,
            headers: headers.clone(),
        }
    }

    async fn send_request(&self, request: &serde_json::Value, _timeout_ms: u64) -> Result<serde_json::Value, String> {
        let mut req_builder = self.client
            .post(&self.url)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req_builder = req_builder.header(k, v);
        }

        let response = req_builder
            .json(request)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

        if let Some(err) = body.get("error") {
            Err(format!("MCP error: {}", err))
        } else {
            Ok(body.get("result").cloned().unwrap_or(serde_json::Value::Null))
        }
    }

    async fn send_notification(&self, notification: &serde_json::Value) -> Result<(), String> {
        let mut req_builder = self.client
            .post(&self.url)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req_builder = req_builder.header(k, v);
        }

        let _ = req_builder.json(notification).send().await;
        Ok(())
    }
}

// ========== Server ==========

struct McpServer {
    name: String,
    transport: Transport,
    tools: Vec<McpTool>,
    info: McpServerInfo,
    timeout: u64,
}

/// Status of an MCP server after config reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub name: String,
    pub status: String,  // "connected", "disabled", "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools_count: Option<usize>,
}

impl McpServer {
    fn get_prefixed_tools(&self) -> Vec<McpTool> {
        self.tools.iter().map(|t| McpTool {
            name: format!("{}_{}", self.name, t.name),
            description: format!("[MCP:{}] {}", self.name, t.description),
            input_schema: t.input_schema.clone(),
        }).collect()
    }
}

// ========== Service ==========

pub struct McpService {
    servers: Arc<RwLock<HashMap<String, McpServer>>>,
}

impl McpService {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load config and connect servers. Returns per-server status.
    pub async fn reload(&self, workspace: Option<&str>) -> Vec<McpServerStatus> {
        let mut merged = Self::load_config_at(&Self::global_config_dir());
        if let Some(ws) = workspace {
            let project = Self::load_config_at(&Self::project_config_dir(ws));
            for (name, cfg) in project.mcp_servers {
                merged.mcp_servers.insert(name, cfg);
            }
        }
        self.init_from_config(&merged, workspace.unwrap_or("")).await
    }

    // ---- Config Loading ----

    fn global_config_dir() -> std::path::PathBuf {
        if cfg!(windows) {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOMEDRIVE").and_then(|hd| {
                    std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp))
                }))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        } else {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        }
        .join(".minimaxcode")
    }

    fn project_config_dir(workspace: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(workspace).join(".minimaxcode")
    }

    fn load_config_at(dir: &std::path::PathBuf) -> McpConfig {
        let path = dir.join("mcp.json");
        if !path.exists() {
            return McpConfig { mcp_servers: HashMap::new() };
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let mut servers_map: HashMap<String, McpServerConfig> = HashMap::new();
                if let Ok(mut raw) = serde_json::from_str::<serde_json::Value>(&content) {
                    // OpenCode format: "mcp" → internal key
                    if raw.get("mcpServers").is_none() {
                        if let Some(mcp_obj) = raw.get("mcp").cloned() {
                            raw["mcpServers"] = mcp_obj;
                        }
                    }
                    if let Some(obj) = raw.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                        for (_key, val) in obj.iter_mut() {
                            // "environment" (OpenCode) → "env"
                            if val.get("env").is_none() {
                                if let Some(env_obj) = val.get("environment").cloned() {
                                    val["env"] = env_obj;
                                }
                            }
                            // Infer type if missing (common in Claude/Cursor format)
                            if val.get("type").is_none() {
                                if val.get("command").is_some() {
                                    val["type"] = serde_json::json!("local");
                                } else if val.get("url").is_some() {
                                    val["type"] = serde_json::json!("remote");
                                }
                            }
                            // Merge "command": "uvx" + "args": [...] → "command": ["uvx", ...]
                            if let Some(cmd_str) = val.get("command").and_then(|c| c.as_str()) {
                                let cmd = cmd_str.to_string();
                                let mut full = vec![cmd];
                                if let Some(args) = val.get("args").and_then(|a| a.as_array()) {
                                    for a in args {
                                        if let Some(s) = a.as_str() {
                                            full.push(s.to_string());
                                        }
                                    }
                                }
                                val["command"] = serde_json::json!(full);
                            }
                        }
                        for (key, val) in obj {
                            if key.starts_with('_') { continue; }
                            match serde_json::from_value::<McpServerConfig>(val.clone()) {
                                Ok(cfg) => { servers_map.insert(key.clone(), cfg); }
                                Err(e) => {
                                    eprintln!("[mcp] Skipping '{}': {}", key, e);
                                }
                            }
                        }
                    }
                }
                eprintln!("[mcp] Loaded config from {} ({} servers)", path.display(), servers_map.len());
                McpConfig { mcp_servers: servers_map }
            }
            Err(e) => {
                eprintln!("[mcp] Failed to read config {}: {}", path.display(), e);
                McpConfig { mcp_servers: HashMap::new() }
            }
        }
    }

    async fn init_from_config(&self, config: &McpConfig, workspace: &str) -> Vec<McpServerStatus> {
        let cwd = if workspace.is_empty() {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        } else {
            workspace.to_string()
        };

        let mut statuses = Vec::new();
        let mut servers = self.servers.write().await;

        // Purge dead servers
        let dead: Vec<String> = servers
            .iter()
            .filter(|(_, s)| !s.transport.is_alive())
            .map(|(n, _)| n.clone())
            .collect();
        for name in &dead {
            servers.remove(name);
            eprintln!("[mcp] Removed dead server: {}", name);
        }

        for (name, server_config) in &config.mcp_servers {
            let enabled = match server_config {
                McpServerConfig::Local { enabled, .. } => *enabled,
                McpServerConfig::Remote { enabled, .. } => *enabled,
            };
            if !enabled {
                statuses.push(McpServerStatus {
                    name: name.clone(),
                    status: "disabled".to_string(),
                    error: None,
                    tools_count: None,
                });
                continue;
            }
            if servers.contains_key(name) {
                // Already connected — report existing status
                if let Some(s) = servers.get(name) {
                    statuses.push(McpServerStatus {
                        name: name.clone(),
                        status: "connected".to_string(),
                        error: None,
                        tools_count: Some(s.tools.len()),
                    });
                }
                continue;
            }
            match Self::connect_server(name, server_config, &cwd).await {
                Ok(server) => {
                    let tc = server.tools.len();
                    eprintln!("[mcp] Connected to {} ({})", name, server.info.name);
                    statuses.push(McpServerStatus {
                        name: name.clone(),
                        status: "connected".to_string(),
                        error: None,
                        tools_count: Some(tc),
                    });
                    servers.insert(name.clone(), server);
                }
                Err(e) => {
                    eprintln!("[mcp] Failed to connect {}: {}", name, e);
                    statuses.push(McpServerStatus {
                        name: name.clone(),
                        status: "failed".to_string(),
                        error: Some(e),
                        tools_count: None,
                    });
                }
            }
        }
        statuses
    }

    async fn connect_server(name: &str, config: &McpServerConfig, cwd: &str) -> Result<McpServer, String> {
        let (transport, timeout) = match config {
            McpServerConfig::Local { command, env, timeout, .. } => {
                // Pre-flight: check if the executable can be found
                let exe = &command[0];
                if which::which(exe).is_err() {
                    return Err(format!(
                        "找不到命令 '{}'——请确认已安装并加入 PATH。当前 PATH: {}",
                        exe,
                        std::env::var("PATH").unwrap_or_default()
                    ));
                }
                let t = StdioTransport::spawn(command, env, cwd)?;
                (Transport::Stdio(t), timeout.unwrap_or(crate::MCP_HTTP_TIMEOUT_SECS * 1000))
            }
            McpServerConfig::Remote { url, headers, timeout, .. } => {
                let ms = timeout.unwrap_or(crate::MCP_HTTP_TIMEOUT_SECS * 1000);
                let t = HttpTransport::new(url, headers, ms);
                (Transport::Http(t), ms)
            }
        };

        // Initialize
        let init_result = transport.send_request(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "minimax-code",
                        "version": "0.1.0"
                    }
                }
            }),
            timeout,
        ).await.map_err(|e| format!("{}{}", e, transport.stderr_snippet()))?;

        let info = McpServerInfo {
            name: init_result
                .get("serverInfo")
                .and_then(|si| si.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(name)
                .to_string(),
            version: init_result
                .get("serverInfo")
                .and_then(|si| si.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
        };

        // Send initialized notification
        transport.send_notification(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })).await.ok();

        // List tools
        let tools_result = transport.send_request(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }),
            timeout,
        ).await.map_err(|e| format!("{}{}", e, transport.stderr_snippet()))?;

        let tools: Vec<McpTool> = tools_result
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(McpTool {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                            input_schema: t.get("inputSchema").cloned().unwrap_or(serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(McpServer {
            name: name.to_string(),
            transport,
            tools,
            info,
            timeout,
        })
    }

    // ---- Public API ----

    pub async fn add_server(&self, name: String, url: String) -> Result<(), String> {
        let mut servers = self.servers.write().await;
        if servers.contains_key(&name) {
            return Err(format!("Server already exists: {}", name));
        }
        let config = McpServerConfig::Remote {
            url,
            headers: HashMap::new(),
            enabled: true,
            timeout: None,
        };
        let server = Self::connect_server(&name, &config, ".").await?;
        servers.insert(name, server);
        Ok(())
    }

    pub async fn remove_server(&self, name: &str) -> bool {
        self.servers.write().await.remove(name).is_some()
    }

    pub async fn list_servers(&self) -> Vec<serde_json::Value> {
        let servers = self.servers.read().await;
        servers
            .values()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "info": s.info,
                    "tools_count": s.tools.len()
                })
            })
            .collect()
    }

    pub async fn get_server_tools(&self, server_name: &str) -> Vec<McpTool> {
        let servers = self.servers.read().await;
        servers
            .get(server_name)
            .map(|s| s.get_prefixed_tools())
            .unwrap_or_default()
    }

    /// Get all MCP tools from all connected servers, with server-name prefix.
    /// Skips dead servers (stdio process crashed).
    pub async fn get_all_tools(&self) -> Vec<McpTool> {
        let servers = self.servers.read().await;
        let mut all = Vec::new();
        for server in servers.values() {
            if !server.transport.is_alive() {
                continue;
            }
            all.extend(server.get_prefixed_tools());
        }
        all
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let servers = self.servers.read().await;
        let server = servers.get(server_name)
            .ok_or_else(|| format!("Server not found: {}", server_name))?;

        if !server.transport.is_alive() {
            return Err(format!("Server '{}' is not running (process exited)", server_name));
        }

        server.transport.send_request(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": args
                }
            }),
            server.timeout,
        ).await
    }

    /// Call any MCP tool by its prefixed name (format: server_toolname).
    pub async fn call_tool_any(&self, full_name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        let servers = self.servers.read().await;
        for (server_name, server) in servers.iter() {
            let prefix = format!("{}_", server_name);
            if !full_name.starts_with(&prefix) {
                continue;
            }
            let tool_name = &full_name[prefix.len()..];
            if !server.tools.iter().any(|t| t.name == tool_name) {
                continue;
            }
            if !server.transport.is_alive() {
                return Err(format!("Server '{}' is not running (process exited)", server_name));
            }
            return server.transport.send_request(
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "tools/call",
                    "params": {
                        "name": tool_name,
                        "arguments": args
                    }
                }),
                server.timeout,
            ).await;
        }
        Err(format!("Tool not found: {}", full_name))
    }
}

impl Default for McpService {
    fn default() -> Self {
        Self::new()
    }
}
