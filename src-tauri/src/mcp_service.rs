// MCP Service — Model Context Protocol client
//
// Supports both local (stdio child process) and remote (HTTP) transports.
// Config loaded from ~/.minimaxcode/mcp.json (global) and {workspace}/.minimaxcode/mcp.json (project).
// Tool naming: {server_name}_{tool_name} to avoid collisions across servers.

use reqwest::blocking::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
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
}

struct HttpTransport {
    url: String,
    client: HttpClient,
    headers: HashMap<String, String>,
}

impl Transport {
    fn send_request(&self, request: &serde_json::Value, timeout_ms: u64) -> Result<serde_json::Value, String> {
        match self {
            Transport::Stdio(t) => t.send_request(request, timeout_ms),
            Transport::Http(t) => t.send_request(request, timeout_ms),
        }
    }

    fn send_notification(&self, notification: &serde_json::Value) -> Result<(), String> {
        match self {
            Transport::Stdio(t) => t.send_notification(notification),
            Transport::Http(t) => t.send_notification(notification),
        }
    }
}

// ---- Stdio Transport ----

impl StdioTransport {
    fn spawn(command: &[String], env: &HashMap<String, String>, cwd: &str) -> Option<Self> {
        let cmd_name = &command[0];
        let cmd_args: Vec<&str> = command[1..].iter().map(|s| s.as_str()).collect();

        let mut cmd = hidden_cmd(cmd_name);
        cmd.args(&cmd_args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Merge environment
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[mcp] Failed to spawn {}: {}", cmd_name, e);
                return None;
            }
        };

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let writer = Mutex::new(BufWriter::new(stdin));
        let pending: Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));

        let reader_pending = pending.clone();
        let reader_alive = alive.clone();
        std::thread::spawn(move || {
            stdio_read_loop(stdout, &reader_pending, &reader_alive);
        });

        Some(Self {
            writer,
            next_id: AtomicU64::new(1),
            pending,
            process: Mutex::new(child),
            alive,
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
        self.pending.lock().unwrap().insert(id, tx);
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
        let _ = self.process.lock().unwrap().kill();
    }
}

fn write_stdio_message(writer: &Mutex<BufWriter<ChildStdin>>, msg: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut w = writer.lock().unwrap();
    w.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    w.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn stdio_read_loop(
    stdout: ChildStdout,
    pending: &Arc<Mutex<HashMap<u64, std::sync::mpsc::Sender<serde_json::Value>>>>,
    alive: &Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let msg = match read_stdio_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(e) => {
                eprintln!("[mcp] Stdio read error: {}", e);
                break;
            }
        };
        if let Some(id) = msg.get("id").and_then(|i| i.as_u64()) {
            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                let _ = tx.send(msg);
            }
        }
        // Notifications (no id) are ignored for now
    }
    alive.store(false, Ordering::SeqCst);
    eprintln!("[mcp] Stdio reader thread exited");
}

fn read_stdio_message(reader: &mut BufReader<ChildStdout>) -> Result<Option<serde_json::Value>, String> {
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

    serde_json::from_slice(&body).map_err(|e| format!("JSON parse: {}", e))
}

// ---- HTTP Transport ----

impl HttpTransport {
    fn new(url: &str, headers: &HashMap<String, String>) -> Self {
        Self {
            url: url.to_string(),
            client: HttpClient::new(),
            headers: headers.clone(),
        }
    }

    fn send_request(&self, request: &serde_json::Value, _timeout_ms: u64) -> Result<serde_json::Value, String> {
        let mut req_builder = self.client
            .post(&self.url)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req_builder = req_builder.header(k, v);
        }

        let response = req_builder
            .json(request)
            .send()
            .map_err(|e| e.to_string())?;

        let body: serde_json::Value = response.json().map_err(|e| e.to_string())?;

        if let Some(err) = body.get("error") {
            Err(format!("MCP error: {}", err))
        } else {
            Ok(body.get("result").cloned().unwrap_or(serde_json::Value::Null))
        }
    }

    fn send_notification(&self, notification: &serde_json::Value) -> Result<(), String> {
        let mut req_builder = self.client
            .post(&self.url)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req_builder = req_builder.header(k, v);
        }

        // Fire and forget
        let _ = req_builder.json(notification).send();
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

    /// Load config and connect servers. Call from async context.
    pub async fn reload(&self, workspace: Option<&str>) {
        let mut merged = Self::load_config_at(&Self::global_config_dir());
        if let Some(ws) = workspace {
            let project = Self::load_config_at(&Self::project_config_dir(ws));
            for (name, cfg) in project.mcp_servers {
                merged.mcp_servers.insert(name, cfg);
            }
        }
        self.init_from_config(&merged, workspace.unwrap_or("")).await;
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
                match serde_json::from_str::<McpConfig>(&content) {
                    Ok(cfg) => {
                        eprintln!("[mcp] Loaded config from {}", path.display());
                        cfg
                    }
                    Err(e) => {
                        eprintln!("[mcp] Failed to parse config {}: {}", path.display(), e);
                        McpConfig { mcp_servers: HashMap::new() }
                    }
                }
            }
            Err(e) => {
                eprintln!("[mcp] Failed to read config {}: {}", path.display(), e);
                McpConfig { mcp_servers: HashMap::new() }
            }
        }
    }

    async fn init_from_config(&self, config: &McpConfig, workspace: &str) {
        let cwd = if workspace.is_empty() {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        } else {
            workspace.to_string()
        };

        let mut servers = self.servers.write().await;
        for (name, server_config) in &config.mcp_servers {
            let enabled = match server_config {
                McpServerConfig::Local { enabled, .. } => *enabled,
                McpServerConfig::Remote { enabled, .. } => *enabled,
            };
            if !enabled || servers.contains_key(name) {
                continue;
            }
            match Self::connect_server(name, server_config, &cwd) {
                Ok(server) => {
                    eprintln!("[mcp] Connected to {} ({})", name, server.info.name);
                    servers.insert(name.clone(), server);
                }
                Err(e) => {
                    eprintln!("[mcp] Failed to connect {}: {}", name, e);
                }
            }
        }
    }

    fn connect_server(name: &str, config: &McpServerConfig, cwd: &str) -> Result<McpServer, String> {
        let (transport, timeout) = match config {
            McpServerConfig::Local { command, env, timeout, .. } => {
                let t = StdioTransport::spawn(command, env, cwd)
                    .ok_or_else(|| format!("Failed to spawn: {:?}", command))?;
                (Transport::Stdio(t), timeout.unwrap_or(crate::MCP_HTTP_TIMEOUT_SECS * 1000))
            }
            McpServerConfig::Remote { url, headers, timeout, .. } => {
                let t = HttpTransport::new(url, headers);
                (Transport::Http(t), timeout.unwrap_or(crate::MCP_HTTP_TIMEOUT_SECS * 1000))
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
        )?;

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
        })).ok();

        // List tools
        let tools_result = transport.send_request(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }),
            timeout,
        )?;

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
        let server = Self::connect_server(&name, &config, ".")?;
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
    pub async fn get_all_tools(&self) -> Vec<McpTool> {
        let servers = self.servers.read().await;
        let mut all = Vec::new();
        for server in servers.values() {
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
        let (timeout, server_exists) = {
            let servers = self.servers.read().await;
            servers.get(server_name).map(|s| (s.timeout, true)).unwrap_or((crate::MCP_HTTP_TIMEOUT_SECS * 1000, false))
        };
        if !server_exists {
            return Err(format!("Server not found: {}", server_name));
        }

        // Re-acquire read lock to get transport reference
        let servers = self.servers.read().await;
        let server = servers.get(server_name).ok_or_else(|| format!("Server not found: {}", server_name))?;

        server.transport.send_request(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": args
                }
            }),
            timeout,
        )
    }

    /// Call any MCP tool by its prefixed name (format: server_toolname).
    pub async fn call_tool_any(&self, full_name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        let servers = self.servers.read().await;
        for (server_name, server) in servers.iter() {
            let prefix = format!("{}_", server_name);
            if full_name.starts_with(&prefix) {
                let tool_name = &full_name[prefix.len()..];
                // Verify the tool exists
                if server.tools.iter().any(|t| t.name == tool_name) {
                    let server_name = server_name.clone();
                    let tool_name = tool_name.to_string();
                    let timeout = server.timeout;
                    drop(servers);

                    let servers = self.servers.read().await;
                    let server = servers.get(&server_name)
                        .ok_or_else(|| format!("Server disappeared: {}", server_name))?;

                    return server.transport.send_request(
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "tools/call",
                            "params": {
                                "name": tool_name,
                                "arguments": args
                            }
                        }),
                        timeout,
                    );
                }
            }
        }
        Err(format!("Tool not found: {}", full_name))
    }
}

impl Default for McpService {
    fn default() -> Self {
        Self::new()
    }
}
