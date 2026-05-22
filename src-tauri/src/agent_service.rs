// Agent Service - Rust Implementation for AI Agent Streaming
//
// Provides:
// - MiniMax API streaming (via reqwest)
// - Tool execution
// - Message history management
// - Interleaved Thinking support

use base64::Engine as _;
use futures_util::StreamExt;
use futures_util::future::join_all;
use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};

use crate::code_graph::CodeGraph;
use crate::context_compressor::{compress_context, estimate_tokens};
use crate::lsp_manager::LspManager;
use crate::lsp_types::format_diagnostics;
use crate::mcp_service::McpService;
use crate::permission::{PermissionService, PermissionAction, PermissionRequest};
use crate::skill_service::SkillService;
use crate::system_prompts::{ACE_SYSTEM, EXPLORE_SYSTEM, FRONT_SYSTEM, PLAN_SYSTEM, REVIEW_SYSTEM, WORK_SYSTEM};
use crate::{DEFAULT_API_URL, SEARCH_TIMEOUT_SECS, VLM_TIMEOUT_SECS};

// ========== Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<String>,  // JSON string of tool_calls array
    #[serde(default)]
    pub thinking: Option<String>,  // thinking content
    #[serde(default)]
    pub raw_json: Option<String>,  // full JSON of content block array for cache preservation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { content: String, thinking: String },
    #[serde(rename = "tool_start")]
    ToolStart { tool: String, tool_id: String, input: serde_json::Value },
    #[serde(rename = "tool_end")]
    ToolEnd { tool: String, tool_id: String, result: String },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "error")]
    Error { content: String },
    #[serde(rename = "cache_usage")]
    CacheUsage { cache_hit_tokens: u64, cache_miss_tokens: u64, cache_hit_ratio: f64 },
    #[serde(rename = "token_usage")]
    TokenUsage { estimated_tokens: usize, context_window: usize, usage_pct: f64 },
}

// ========== Agent Service ==========

type PendingAsks = Arc<StdMutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>>;

/// Save an api_message (with structured content blocks) to the chat_message table.
/// Stores display text in `content` and the full JSON block array in `raw_json`.
fn save_api_message(db: &Arc<StdMutex<Connection>>, session_id: i64, message: &serde_json::Value) {
    let role = message["role"].as_str().unwrap_or("user");
    let content_val = &message["content"];

    // Extract display text from content (string or array of blocks)
    let display_text = match content_val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            blocks.iter()
                .filter(|b| b["type"] == "text")
                .map(|b| b["text"].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("")
        }
        _ => String::new(),
    };

    // Full JSON of the content for cache-identical reconstruction
    let raw_json = serde_json::to_string(content_val).unwrap_or_default();

    // Extract thinking from blocks for the thinking column
    let thinking = match content_val {
        serde_json::Value::Array(blocks) => {
            let t: String = blocks.iter()
                .filter(|b| b["type"] == "thinking")
                .map(|b| b["thinking"].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("");
            if t.is_empty() { None } else { Some(t) }
        }
        _ => None,
    };

    // Extract tool_calls from blocks for the tool_calls column
    let tool_calls = match content_val {
        serde_json::Value::Array(blocks) => {
            let tc: Vec<serde_json::Value> = blocks.iter()
                .filter(|b| b["type"] == "tool_use")
                .map(|b| json!({
                    "id": b["id"],
                    "type": "function",
                    "function": {
                        "name": b["name"],
                        "arguments": serde_json::to_string(&b["input"]).unwrap_or_default()
                    }
                }))
                .collect();
            if tc.is_empty() { None } else { Some(serde_json::to_string(&tc).unwrap_or_default()) }
        }
        _ => None,
    };

    if let Ok(conn) = db.lock() {
        let _ = conn.execute(
            "INSERT INTO chat_message (session_id, role, content, tool_calls, thinking, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![session_id, role, display_text, tool_calls, thinking, raw_json],
        );
    }
}

/// Mark the last message's last content block with cache_control for incremental
/// multi-turn caching. Converts string content to block array format if needed.
fn mark_last_message_for_cache(msgs: &mut Vec<serde_json::Value>) {
    if let Some(last_msg) = msgs.last_mut() {
        let content = &mut last_msg["content"];
        if content.is_string() {
            let text = content.as_str().unwrap_or("").to_string();
            *content = json!([{
                "type": "text",
                "text": text,
                "cache_control": {"type": "ephemeral"}
            }]);
        } else if content.is_array() {
            if let Some(arr) = content.as_array_mut() {
                if let Some(last_block) = arr.last_mut() {
                    last_block["cache_control"] = json!({"type": "ephemeral"});
                }
            }
        }
    }
}

/// Save original file content before modification for undo support.
fn save_file_snapshot(db: &Arc<StdMutex<Connection>>, session_id: i64, file_path: &str) {
    if let Ok(conn) = db.lock() {
        // Keep only the earliest (original) snapshot per file for correct rewind
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM file_snapshot WHERE session_id = ?1 AND file_path = ?2",
            rusqlite::params![session_id, file_path],
            |row| row.get(0),
        ).unwrap_or(false);
        if exists {
            return;
        }

        let original = match std::fs::read(file_path) {
            Ok(bytes) => {
                match String::from_utf8(bytes.clone()) {
                    Ok(s) if is_printable_text(&s) => Some(s),
                    _ => Some(format!("hex:{}", hex_encode(&bytes))),
                }
            }
            Err(_) => None, // file doesn't exist yet
        };
        let _ = conn.execute(
            "INSERT INTO file_snapshot (session_id, file_path, original_content) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, file_path, original],
        );
    }
}

fn is_printable_text(s: &str) -> bool {
    for ch in s.chars() {
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            return false;
        }
    }
    true
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub(crate) fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i+2], 16).ok())
        .collect()
}

/// Collect all file paths in a directory tree (non-recursive symlink safe, depth-limited).
fn collect_dir_files(dir: &std::path::Path, out: &mut Vec<String>) {
    use std::fs;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Limit depth to avoid huge snapshots (max 4 levels)
                if path.components().count() <= dir.components().count() + 4 {
                    collect_dir_files(&path, out);
                }
            } else if path.is_file() {
                if let Some(s) = path.to_str() {
                    out.push(s.to_string());
                }
            }
        }
    }
}

/// Snapshot all text files in a workspace before a destructive command.
/// Limited to 200 files and 10MB total to avoid performance issues.
fn snapshot_workspace_files(db: &Arc<StdMutex<Connection>>, session_id: i64, cwd: &str) {
    let mut files: Vec<String> = Vec::new();
    collect_dir_files(std::path::Path::new(cwd), &mut files);
    let mut total: usize = 0;
    const MAX_FILES: usize = 200;
    const MAX_BYTES: usize = 10_000_000;
    for f in files.iter().take(MAX_FILES) {
        if let Ok(meta) = std::fs::metadata(f) {
            total += meta.len() as usize;
            if total > MAX_BYTES { break; }
        }
        save_file_snapshot(db, session_id, f);
    }
}

/// Write snapshot content to a file path, decoding hex if needed.
/// Returns true on success.
pub(crate) fn restore_snapshot_file(path: &str, content: Option<&str>) -> bool {
    match content {
        None => {
            // File didn't exist before — remove it
            let p = std::path::Path::new(path);
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else if p.exists() {
                let _ = std::fs::remove_file(path);
            }
            true
        }
        Some(c) => {
            if let Some(hex) = c.strip_prefix("hex:") {
                let bytes = hex_decode(hex);
                std::fs::write(path, &bytes).is_ok()
            } else {
                std::fs::write(path, c).is_ok()
            }
        }
    }
}

pub struct AgentService {
    api_url: String,
    messages_path: String,
    model: String,
    context_window: usize,
    provider: String,
    api_key: Arc<Mutex<String>>,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    code_graph: Arc<StdMutex<CodeGraph>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
}

impl Clone for AgentService {
    fn clone(&self) -> Self {
        Self {
            api_url: self.api_url.clone(),
            messages_path: self.messages_path.clone(),
            model: self.model.clone(),
            context_window: self.context_window,
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            skill_service: self.skill_service.clone(),
            mcp_service: self.mcp_service.clone(),
            db: self.db.clone(),
            code_graph: self.code_graph.clone(),
            lsp_manager: self.lsp_manager.clone(),
            permission_service: self.permission_service.clone(),
            pending_asks: self.pending_asks.clone(),
        }
    }
}

impl AgentService {
    pub fn new(api_key: String, api_url: String, messages_path: String, model: String, context_window: usize, provider: String, skill_service: Arc<SkillService>, mcp_service: Arc<RwLock<McpService>>, db: Arc<StdMutex<Connection>>, code_graph: Arc<StdMutex<CodeGraph>>, lsp_manager: Arc<StdMutex<Option<LspManager>>>, permission_service: Arc<StdMutex<PermissionService>>, pending_asks: PendingAsks) -> Self {
        Self {
            api_url,
            messages_path,
            model,
            context_window,
            provider,
            api_key: Arc::new(Mutex::new(api_key)),
            skill_service,
            mcp_service,
            db,
            code_graph,
            lsp_manager,
            permission_service,
            pending_asks,
        }
    }

    pub async fn stream_chat(
        &self,
        agent_type: &str,
        messages: Vec<Message>,
        _system: Option<String>,
        workspace: Option<String>,
        app_handle: AppHandle,
        session_id: i64,
    ) {
        let api_key = self.api_key.lock().await.clone();
        let skill_service = self.skill_service.clone();
        let mcp_service = self.mcp_service.clone();
        let lsp_manager = self.lsp_manager.clone();
        let permission_service = self.permission_service.clone();
        let pending_asks = self.pending_asks.clone();
        let app = app_handle.clone();
        let session_key = format!("agent_stream_{}", session_id);
        eprintln!("[stream_chat] session_key: {}", session_key);

        // Load project skills if workspace is set
        if let Some(ref ws) = workspace {
            skill_service.load_project_skills(ws).await;
            let mcp = mcp_service.read().await;
            mcp.reload(Some(ws)).await;
        }

        // Build system prompt based on agent type
        let base_system = match agent_type {
            "front" => FRONT_SYSTEM,
            "plan" => PLAN_SYSTEM,
            "work" => WORK_SYSTEM,
            "review" => REVIEW_SYSTEM,
            "explore" => EXPLORE_SYSTEM,
            "ace" => ACE_SYSTEM,
            _ => FRONT_SYSTEM,
        };

        // Build immutable system prompt — NEVER mutate after construction.
        // Prefix-based KV caching depends on byte-identical prefix across turns.
        let system_text = match workspace {
            Some(ws) => format!("{}\n\n# 工作目录\n{}", base_system, ws),
            None => base_system.to_string(),
        };

        // System prompt as top-level `system` field (Anthropic format).
        // cache_control only for MiniMax (KV cache). Custom providers may reject it.
        let system_block = if self.provider == "custom" {
            json!({"type": "text", "text": system_text})
        } else {
            json!({"type": "text", "text": system_text, "cache_control": {"type": "ephemeral"}})
        };
        let system_prompt = json!([system_block]);

        let mut tools: Vec<serde_json::Value> = get_agent_tools(agent_type)
            .into_iter()
            .filter(|t| {
                // Custom providers don't have MiniMax search API — agent uses MCP web_search instead
                self.provider != "custom" || t["name"].as_str() != Some("web_search")
            })
            .collect();

        // Append MCP tools from all connected servers (names prefixed as server_tool)
        {
            let mcp = mcp_service.read().await;
            let mcp_tools = mcp.get_all_tools().await;
            let builtin_count = tools.len();
            for t in &mcp_tools {
                tools.push(make_tool(&t.name, &t.description, t.input_schema.clone()));
            }
            if !mcp_tools.is_empty() {
                eprintln!("[stream_chat] Loaded {} MCP tools, total: {}",
                    mcp_tools.len(), builtin_count + mcp_tools.len());
            }
        }

        // Mark last tool with cache_control to cache all tool definitions.
        // Per MiniMax docs: cache_control on the last tool caches ALL preceding tools.
        // Skip for custom providers that may reject cache_control.
        if self.provider != "custom" && !tools.is_empty() {
            let last_idx = tools.len() - 1;
            if let Some(obj) = tools[last_idx].as_object_mut() {
                obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
            }
        }

        // Build messages array (NO system message — system is top-level `system` field).
        let mut api_messages: Vec<serde_json::Value> = Vec::new();
        // Save last user message for skill matching before messages is consumed
        let last_user_msg = messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        // Reconstruct messages from history, using raw_json for cache-identical byte sequences
        for msg in messages {
            let content_val = match msg.raw_json {
                Some(ref raw) => {
                    serde_json::from_str::<serde_json::Value>(raw)
                        .unwrap_or_else(|_| json!(msg.content))
                }
                None => {
                    // Fallback: try to reconstruct structured content from tool_calls/thinking
                    if msg.role == "assistant" && (msg.tool_calls.is_some() || msg.thinking.is_some()) {
                        let mut blocks = Vec::new();
                        if let Some(ref thinking) = msg.thinking {
                            if !thinking.is_empty() {
                                blocks.push(json!({"type": "thinking", "thinking": thinking}));
                            }
                        }
                        if !msg.content.is_empty() {
                            blocks.push(json!({"type": "text", "text": msg.content}));
                        }
                        if let Some(ref tc) = msg.tool_calls {
                            if let Ok(tool_calls) = serde_json::from_str::<Vec<serde_json::Value>>(tc) {
                                for tc in &tool_calls {
                                    blocks.push(json!({
                                        "type": "tool_use",
                                        "id": tc["id"],
                                        "name": tc["function"]["name"],
                                        "input": tc["function"]["arguments"]
                                    }));
                                }
                            }
                        }
                        if blocks.is_empty() {
                            json!(msg.content)
                        } else {
                            json!(blocks)
                        }
                    } else {
                        json!(msg.content)
                    }
                }
            };
            api_messages.push(json!({
                "role": msg.role,
                "content": content_val
            }));
        }

        // Model info as dynamic message — keeps system prompt immutable for cache.
        // Placed before skill context so the stable model_name prefix can be cached.
        api_messages.push(json!({
            "role": "user",
            "content": format!("当前模型: {}", self.model)
        }));

        // Proactive skill matching: append as a separate user message AFTER history.
        // This preserves the immutable system prefix — critical for cache hits.
        let matched_skills = skill_service.match_skills(&last_user_msg, 5).await;
        let relevant: Vec<_> = matched_skills.iter().filter(|m| m.score > 0.15).collect();
        if !relevant.is_empty() {
            let mut skill_context = String::from("## 匹配到的技能\n\n");
            skill_context.push_str("以下技能可能与你的任务相关。如需加载某个技能的完整操作指令，调用 `skill` 工具传入技能名称：\n\n");
            for m in &relevant {
                skill_context.push_str(&format!("- **{}** (匹配度: {:.0}%): {}\n",
                    m.name, m.score * 100.0, m.description));
            }
            api_messages.push(json!({
                "role": "user",
                "content": skill_context
            }));
            eprintln!("[stream_chat] Appended {} matched skills as separate message (prefix preserved)", relevant.len());
        }

        // Mark last message's last content block with cache_control for incremental
        // multi-turn caching. Per MiniMax docs, this caches the entire conversation prefix.
        // Only done once before the loop — subsequent loop iterations benefit from prefix cache.
        // Skip for custom providers that may reject cache_control.
        if self.provider != "custom" {
            mark_last_message_for_cache(&mut api_messages);
        }

        // Main loop: continue until stop_reason is not "tool_use"
        loop {
            // Compress context when approaching token limit (80% of context window)
            compress_context(agent_type, &mut api_messages, self.context_window);

            // Emit token usage for context window display
            let est = estimate_tokens(&api_messages);
            let usage_pct = (est as f64 / self.context_window as f64) * 100.0;
            let _ = app.emit(&session_key, StreamEvent::TokenUsage {
                estimated_tokens: est,
                context_window: self.context_window,
                usage_pct,
            });

            let request_body = json!({
                "model": self.model,
                "system": system_prompt,
                "messages": api_messages,
                "max_tokens": 16384,
                "temperature": 1,
                "stream": true,
                "tools": tools,
            });
            let request_body_str = serde_json::to_string(&request_body).unwrap();

            eprintln!("[stream_chat] Model: {}, Ctx: {}K, Messages: {}", self.model, self.context_window / 1000, api_messages.len());

            // Send request
            let client = Client::new();
            let response = match client
                .post(format!("{}{}", self.api_url, self.messages_path))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .body(request_body_str)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[stream_chat] Request failed: {}", e);
                    let ev = StreamEvent::Error { content: format!("Request failed: {}", e) };
                    let _ = app.emit(&session_key, ev);
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                eprintln!("[stream_chat] API error {}: {}", status, err_text);
                eprintln!("[stream_chat] URL: {}{}", self.api_url, self.messages_path);
                let ev = StreamEvent::Error { content: format!("API error {}: {}", status, err_text) };
                let _ = app.emit(&session_key, ev);
                return;
            }

            // Process SSE stream and collect result
            let (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results) = process_sse_stream(
                response.bytes_stream(),
                app.clone(),
                session_key.clone(),
                session_id,
                api_key.clone(),
                self.api_url.clone(),
                self.model.clone(),
                self.provider.clone(),
                skill_service.clone(),
                mcp_service.clone(),
                self.db.clone(),
                self.code_graph.clone(),
                lsp_manager.clone(),
                permission_service.clone(),
                pending_asks.clone(),
                &mut api_messages,
            ).await;

            eprintln!("[stream_chat] Model: {}, stop_reason: {:?}, thinking: {} chars, text: {} chars, tool_uses: {}", self.model,
                stop_reason, assistant_thinking.len(), assistant_text.len(), tool_uses.len());

            // If stop_reason is not "tool_use", we're done
            if stop_reason.as_deref() != Some("tool_use") {
                // Save final assistant message to DB (backend handles persistence for cache)
                if !assistant_thinking.is_empty() || !assistant_text.is_empty() {
                    let mut final_content = Vec::new();
                    if !assistant_thinking.is_empty() {
                        final_content.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                    }
                    if !assistant_text.is_empty() {
                        final_content.push(json!({"type": "text", "text": assistant_text}));
                    }
                    let final_msg = json!({
                        "role": "assistant",
                        "content": final_content
                    });
                    save_api_message(&self.db, session_id, &final_msg);
                    api_messages.push(final_msg);
                }
                let ev = StreamEvent::Done;
                let _ = app.emit(&session_key, ev);
                break;
            }

            // stop_reason was "tool_use"
            // 1) Add assistant message (thinking + text + tool_use blocks) to api_messages
            // CRITICAL: must include ALL content blocks to preserve Interleaved Thinking chain.
            if !assistant_thinking.is_empty() || !assistant_text.is_empty() || !tool_uses.is_empty() {
                let mut assistant_content = Vec::new();
                if !assistant_thinking.is_empty() {
                    assistant_content.push(json!({"type": "thinking", "thinking": assistant_thinking}));
                }
                if !assistant_text.is_empty() {
                    assistant_content.push(json!({"type": "text", "text": assistant_text}));
                }
                for (tool_id, tool_name, tool_input) in &tool_uses {
                    let input_json: serde_json::Value = serde_json::from_str(tool_input).unwrap_or(json!({}));
                    assistant_content.push(json!({
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": input_json
                    }));
                }
                let assistant_msg = json!({
                    "role": "assistant",
                    "content": assistant_content
                });
                // Persist to DB for cache-identical reconstruction on next turn
                save_api_message(&self.db, session_id, &assistant_msg);
                api_messages.push(assistant_msg);
            }

            // 2) Add tool results to messages and loop
            for (_tool_name, tool_id, result) in tool_results {
                let tool_msg = json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": result
                    }]
                });
                // Persist to DB for cache-identical reconstruction on next turn
                save_api_message(&self.db, session_id, &tool_msg);
                api_messages.push(tool_msg);
            }
        }
    }
}

// Process SSE stream from MiniMax API
// Returns (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results)
// tool_uses: Vec<(tool_id, tool_name, input_accumulated)>
// tool_results: Vec<(tool_name, tool_id, result)>
async fn process_sse_stream(
    stream: impl StreamExt<Item = Result<bytes::Bytes, reqwest::Error>>,
    app: AppHandle,
    session_key: String,
    session_id: i64,
    api_key: String,
    api_url: String,
    model: String,
    provider: String,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    code_graph: Arc<StdMutex<CodeGraph>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    _api_messages: &mut Vec<serde_json::Value>,
) -> (Option<String>, String, String, Vec<(String, String, String)>, Vec<(String, String, String)>) {
    eprintln!("[process_sse_stream] Starting with session_key: {}", session_key);
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_input = String::new();
    let mut tool_inputs: HashMap<String, (String, String)> = HashMap::new();
    let mut stop_reason: Option<String> = None;
    let mut tool_results: Vec<(String, String, String)> = Vec::new();
    let mut assistant_text = String::new();
    let mut assistant_thinking = String::new();

    // Cache usage tracking (prefix-based KV cache)
    let mut cache_hit_tokens: u64 = 0;
    let mut cache_miss_tokens: u64 = 0;

    // Text emission buffer — merge rapid deltas to reduce IPC overhead.
    // 8ms balances IPC overhead against smoothness: emits ~120x/sec max, imperceptible.
    let mut last_emit = std::time::Instant::now();
    let emit_interval = std::time::Duration::from_millis(8);
    let mut pending_text = String::new();
    let mut pending_thinking = String::new();

    futures_util::pin_mut!(stream);

    while let Some(item) = stream.next().await {
        match item {
            Ok(bytes) => {
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                                let prev_text_len = assistant_text.len();
                                let prev_think_len = assistant_thinking.len();

                                if let Some(reason) = handle_sse_event(
                                    &event,
                                    &mut current_tool_id,
                                    &mut current_tool_name,
                                    &mut current_tool_input,
                                    &mut tool_inputs,
                                    &mut assistant_text,
                                    &mut assistant_thinking,
                                ) {
                                    stop_reason = Some(reason);
                                }

                                // Capture cache usage from message_delta events.
                                // MiniMax fields: cache_read (hit), cache_creation (write), input_tokens (post-breakpoint)
                                if event["type"] == "message_delta" {
                                    if let Some(usage) = event["usage"].as_object() {
                                        let hit = usage.get("cache_read_input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        let create = usage.get("cache_creation_input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        let input = usage.get("input_tokens")
                                            .and_then(|v| v.as_u64()).unwrap_or(0);
                                        // cache_read = existing cache reused
                                        // cache_creation + input_tokens = tokens not cached (just written + post-breakpoint)
                                        cache_hit_tokens += hit;
                                        cache_miss_tokens += create + input;
                                        eprintln!(
                                            "[cache] read={}, create={}, input={}, cumulative hit={} miss={}",
                                            hit, create, input, cache_hit_tokens, cache_miss_tokens
                                        );
                                    }
                                }

                                // Accumulate new text/thinking since last emit
                                if assistant_text.len() > prev_text_len {
                                    pending_text.push_str(&assistant_text[prev_text_len..]);
                                }
                                if assistant_thinking.len() > prev_think_len {
                                    pending_thinking.push_str(&assistant_thinking[prev_think_len..]);
                                }

                                // Emit if interval passed or this is a non-text event
                                let is_non_text = event["type"] != "content_block_delta";
                                let elapsed = last_emit.elapsed();
                                if (is_non_text && (!pending_text.is_empty() || !pending_thinking.is_empty()))
                                    || elapsed >= emit_interval
                                {
                                    if !pending_text.is_empty() || !pending_thinking.is_empty() {
                                        let ev = StreamEvent::ContentBlockDelta {
                                            content: std::mem::take(&mut pending_text),
                                            thinking: std::mem::take(&mut pending_thinking),
                                        };
                                        let _ = app.emit(&session_key, ev);
                                        last_emit = std::time::Instant::now();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Flush any remaining text before error
                if !pending_text.is_empty() || !pending_thinking.is_empty() {
                    let ev = StreamEvent::ContentBlockDelta {
                        content: std::mem::take(&mut pending_text),
                        thinking: std::mem::take(&mut pending_thinking),
                    };
                    let _ = app.emit(&session_key, ev);
                }
                let ev = StreamEvent::Error { content: format!("Stream error: {}", e) };
                let _ = app.emit(&session_key, ev);
                break;
            }
        }
    }

    // Flush any remaining buffered text
    if !pending_text.is_empty() || !pending_thinking.is_empty() {
        let ev = StreamEvent::ContentBlockDelta {
            content: std::mem::take(&mut pending_text),
            thinking: std::mem::take(&mut pending_thinking),
        };
        let _ = app.emit(&session_key, ev);
    }

    // Emit cache usage stats for this turn
    if cache_hit_tokens > 0 || cache_miss_tokens > 0 {
        let total = cache_hit_tokens + cache_miss_tokens;
        let ratio = if total > 0 { cache_hit_tokens as f64 / total as f64 } else { 0.0 };
        let _ = app.emit(&session_key, StreamEvent::CacheUsage {
            cache_hit_tokens,
            cache_miss_tokens,
            cache_hit_ratio: ratio,
        });
        eprintln!(
            "[cache] turn stats: hit={}, miss={}, ratio={:.2}%",
            cache_hit_tokens, cache_miss_tokens, ratio * 100.0
        );
    }

    // Build tool_uses list from collected tool_inputs
    let tool_uses: Vec<(String, String, String)> = tool_inputs
        .into_iter()
        .map(|(tool_id, (tool_name, input))| (tool_id, tool_name, input))
        .collect();

    // Storm breaker: track repeated identical tool calls within this turn
    let mut storm_counter: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    // Execute tools with concurrent chunking (parallel-safe tools run together)
    if stop_reason.as_deref() == Some("tool_use") {
        // Filter out storm tools (same tool + same input > 2 times)
        let storm_tool_uses: Vec<(String, String, String)> = tool_uses.iter().filter(|(_, name, input)| {
            let key = format!("{}|{}", name, input);
            let count = storm_counter.entry(key).or_insert(0);
            *count += 1;
            *count <= 2
        }).cloned().collect();
        let blocked_count = tool_uses.len() - storm_tool_uses.len();
        if blocked_count > 0 {
            eprintln!("[storm_breaker] Blocked {} repeated tool calls", blocked_count);
        }
        let tool_uses = storm_tool_uses;

        let parallel_max = std::env::var("MINIMAX_PARALLEL_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(16);
        let dispatch_serial = std::env::var("MINIMAX_TOOL_DISPATCH")
            .map(|v| v == "serial")
            .unwrap_or(false);

        let mut idx = 0;
        while idx < tool_uses.len() {
            // Build chunk: consecutive parallel-safe tools up to parallel_max
            let mut chunk: Vec<usize> = Vec::new();
            if !dispatch_serial {
                while idx < tool_uses.len()
                    && chunk.len() < parallel_max
                    && is_parallel_safe(&tool_uses[idx].1)
                {
                    chunk.push(idx);
                    idx += 1;
                }
            }
            // If no parallel-safe could be grouped, take one (non-parallel-safe) alone
            if chunk.is_empty() {
                chunk.push(idx);
                idx += 1;
            }

            // Emit all tool_start events before dispatch (UI shows all at once)
            for &i in &chunk {
                let (_tool_id, tool_name, final_input) = &tool_uses[i];
                let input_json: serde_json::Value =
                    serde_json::from_str(final_input).unwrap_or(json!({}));
                let _ = app.emit(&session_key, StreamEvent::ToolStart {
                    tool: tool_name.clone(),
                    tool_id: _tool_id.clone(),
                    input: input_json,
                });
            }

            // Run chunk concurrently
            let futs: Vec<_> = chunk.iter().map(|&i| {
                let (_tool_id, tool_name, final_input) = &tool_uses[i];
                let tool_name = tool_name.clone();
                let final_input = final_input.clone();
                let api_key = api_key.clone();
                let api_url = api_url.clone();
                let model = model.clone();
                let provider = provider.clone();
                let skill_service = skill_service.clone();
                let mcp_service = mcp_service.clone();
                let db = db.clone();
                let code_graph = code_graph.clone();
                let lsp_manager = lsp_manager.clone();
                let permission_service = permission_service.clone();
                let pending_asks = pending_asks.clone();
                let app = app.clone();
                let sid = session_id;
                tokio::spawn(async move {
                    let result = execute_tool(
                        &tool_name, &final_input, sid,
                        api_key, api_url, model, provider,
                        skill_service, mcp_service,
                        db, code_graph, lsp_manager, permission_service, pending_asks, app,
                    ).await;
                    (i, tool_name, result)
                })
            }).collect();

            let results = join_all(futs).await;

            // Emit tool_end in declared order
            for r in results {
                match r {
                    Ok((i, tool_name, result)) => {
                        let tool_id = tool_uses[i].0.clone();
                        let _ = app.emit(&session_key, StreamEvent::ToolEnd {
                            tool: tool_name.clone(),
                            tool_id: tool_id.clone(),
                            result: result.clone(),
                        });
                        tool_results.push((tool_name, tool_id, result));
                    }
                    Err(e) => {
                        let _ = app.emit(&session_key, StreamEvent::Error {
                            content: format!("Tool join error: {}", e),
                        });
                    }
                }
            }
        }
    }

    (stop_reason, assistant_text, assistant_thinking, tool_uses, tool_results)
}

fn handle_sse_event(
    event: &serde_json::Value,
    current_tool_id: &mut Option<String>,
    current_tool_name: &mut Option<String>,
    current_tool_input: &mut String,
    tool_inputs: &mut HashMap<String, (String, String)>,
    assistant_text: &mut String,
    assistant_thinking: &mut String,
) -> Option<String> {
    let event_type = event["type"].as_str().unwrap_or("");
    eprintln!("[handle_sse_event] event_type: {}", event_type);

    match event_type {
        "content_block_start" => {
            let block = &event["content_block"];
            let block_type = block["type"].as_str().unwrap_or("");
            match block_type {
                "tool_use" => {
                    *current_tool_id = block["id"].as_str().map(|s| s.to_string());
                    *current_tool_name = block["name"].as_str().map(|s| s.to_string());
                    current_tool_input.clear();
                }
                "thinking" => {
                    // Some providers (DeepSeek) send full thinking in content_block_start
                    if let Some(thinking) = block["thinking"].as_str() {
                        if !thinking.is_empty() {
                            eprintln!("[sse] thinking block_start: {} chars", thinking.len());
                            assistant_thinking.push_str(thinking);
                        }
                    }
                }
                _ => {}
            }
            None
        }
        "content_block_delta" => {
            let delta = &event["delta"];

            // Accumulate text (emission handled by process_sse_stream buffer)
            if let Some(text) = delta["text"].as_str() {
                if !text.is_empty() {
                    assistant_text.push_str(text);
                }
            }

            // Accumulate thinking (streaming delta)
            if let Some(thinking) = delta["thinking"].as_str() {
                if !thinking.is_empty() {
                    eprintln!("[sse] thinking delta: {} chars", thinking.len());
                    assistant_thinking.push_str(thinking);
                }
            }

            // Tool input delta (MiniMax uses partial_json field)
            if let Some(input) = delta["partial_json"].as_str() {
                if let Some(ref tool_id) = *current_tool_id {
                    current_tool_input.push_str(input);
                    let tool_name = current_tool_name.clone().unwrap_or_default();
                    tool_inputs.insert(tool_id.clone(), (tool_name, current_tool_input.clone()));
                }
            }
            None
        }
        "message_delta" => {
            if let Some(stop_reason) = event["delta"]["stop_reason"].as_str() {
                // Don't execute tools here - collect all tool_inputs and execute after stream ends
                // This ensures all tools are properly accumulated
                Some(stop_reason.to_string())
            } else {
                None
            }
        }
        "message_stop" => {
            // Don't emit Done here - Done is emitted by stream_chat
            // when the loop actually breaks (stop_reason != "tool_use")
            None
        }
        _ => None
    }
}

// ========== Tool Execution ==========

async fn execute_tool(
    tool_name: &str,
    input: &str,
    session_id: i64,
    api_key: String,
    api_url: String,
    model: String,
    provider: String,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    code_graph: Arc<StdMutex<CodeGraph>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    app_handle: AppHandle,
) -> String {
    let params: serde_json::Value = serde_json::from_str(input).unwrap_or(json!({}));

    // --- Permission Check ---
    {
        let file_path = params["path"].as_str()
            .or_else(|| params["file_path"].as_str())
            .or_else(|| params["target"].as_str());
        let command = params["command"].as_str();
        let reason = tool_reason(tool_name, file_path, command);

        let verdict = {
            permission_service.lock().unwrap().evaluate(tool_name, file_path, command)
        };
        match verdict {
            None => {
                // Need confirmation — emit event and wait
                let (perm_id, rx) = {
                    permission_service.lock().unwrap().register_pending()
                };
                let req = PermissionRequest {
                    id: perm_id.clone(),
                    tool: tool_name.to_string(),
                    file: file_path.map(|s| s.to_string()),
                    command: command.map(|s| s.to_string()),
                    reason: reason.clone(),
                };
                let _ = app_handle.emit("permission_asked", &req);
                match rx.await {
                    Ok(PermissionAction::Allow) => {
                        eprintln!("[perm] {} allowed by user", tool_name);
                    }
                    Ok(PermissionAction::Deny) | Err(_) => {
                        eprintln!("[perm] {} denied", tool_name);
                        return format!("Permission denied: {}", reason);
                    }
                }
            }
            Some(Ok(())) => {
                // Allowed silently
            }
            Some(Err(msg)) => {
                return format!("Blocked: {}", msg);
            }
        }
    }

    // Extract file path for auto-touch BEFORE params is potentially moved in the match
    let auto_touch_path: Option<String> = match tool_name {
        "write_file" | "delete_file" | "copy_file" | "move_file" | "edit_file" | "create_directory" =>
            params["path"].as_str().map(|s| normalize_file_path(s)),
        _ => None,
    };

    // Save file snapshots before modification for undo support
    match tool_name {
        "write_file" | "edit_file" | "delete_file" => {
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(p));
            }
        }
        "write_files" | "modify_files" => {
            if let Some(files) = params["files"].as_array() {
                for f in files.iter().filter_map(|f| f.as_object()) {
                    if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
                        save_file_snapshot(&db, session_id, &normalize_file_path(p));
                    }
                }
            }
        }
        "move_file" => {
            if let Some(src) = params["source"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(src));
            }
            if let Some(dst) = params["destination"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(dst));
            }
        }
        "copy_file" => {
            if let Some(dst) = params["destination"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(dst));
            }
        }
        "multi_edit" | "edit_lines" => {
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(p));
            }
        }
        "create_dir" => {
            // Snapshot the directory itself (NULL = didn't exist, undo will delete it)
            if let Some(p) = params["path"].as_str() {
                save_file_snapshot(&db, session_id, &normalize_file_path(p));
            }
        }
        "remove_path" => {
            if let Some(p) = params["path"].as_str() {
                let np = normalize_file_path(p);
                let path = std::path::Path::new(&np);
                if path.is_dir() {
                    // Snapshot all files in the directory tree before deletion
                    if std::fs::read_dir(path).is_ok() {
                        let mut files: Vec<String> = Vec::new();
                        collect_dir_files(path, &mut files);
                        for f in &files {
                            save_file_snapshot(&db, session_id, f);
                        }
                    }
                }
                // Snapshot the path itself (file or dir)
                save_file_snapshot(&db, session_id, &np);
            }
        }
        "run_command" | "run_tests" | "run_background" => {
            // Snapshot workspace files before command execution
            if let Some(ref cwd) = params["cwd"].as_str().map(|s| s.to_string())
                .or_else(|| params["path"].as_str().map(|s| s.to_string()))
            {
                snapshot_workspace_files(&db, session_id, cwd);
            }
        }
        _ => {}
    }

    let result = match tool_name {
        "list_dir" => tool_list_dir(&params).await,
        "read_file" => tool_read_file(&params).await,
        "read_files" => tool_read_files(&params).await,
        "git_status" => tool_git_status(&params).await,
        "git_log" => tool_git_log(&params).await,
        "git_diff" => tool_git_diff(&params).await,
        "git_commit" => tool_git_commit(&params).await,
        "git_branch" => tool_git_branch(&params).await,
        "git_checkout" => tool_git_checkout(&params).await,
        "git_stash" => tool_git_stash(&params).await,
        "git_stash_pop" => tool_git_stash_pop(&params).await,
        "search_in_dir" => tool_search_in_dir(&params).await,
        "get_env_info" => tool_get_env_info(&params).await,
        "analyze_project_structure" => tool_analyze_project_structure(&params).await,
        "run_command" => tool_run_command(&params).await,
        "write_file" => tool_write_file(&params).await,
        "write_files" => tool_write_files(&params).await,
        "find_replace_in_files" => tool_find_replace_in_files(&params).await,
        "modify_files" => tool_modify_files(&params).await,
        "get_file_info" => tool_get_file_info(&params).await,
        "directory_tree" => tool_directory_tree(&params).await,
        "glob" => tool_glob(&params).await,
        "search_files" => tool_search_files(&params).await,
        "edit_file" => tool_edit_file(&params).await,
        "edit_lines" => tool_edit_lines(&params).await,
        "multi_edit" => tool_multi_edit(&params).await,
        "create_directory" => tool_create_directory(&params).await,
        "move_file" => tool_move_file(&params).await,
        "delete_file" => tool_delete_file(&params).await,
        "copy_file" => tool_copy_file_fn(&params).await,
        "web_fetch" => tool_web_fetch(params.clone()).await,
        "run_background" => tool_run_background(&params).await,
        "job_output" => tool_job_output(&params).await,
        "list_jobs" => tool_list_jobs(&params).await,
        "run_tests" => tool_run_tests(&params).await,
        "spawn_process" => tool_spawn_process(&params).await,
        "kill_process" => tool_kill_process(&params).await,
        "web_search" => tool_web_search(params.clone(), api_key.clone(), api_url.clone(), provider.clone()).await,
        "understand_image" => tool_understand_image(params.clone(), api_key.clone(), api_url.clone(), model.clone(), provider.clone()).await,
        "skill" => tool_skill(tool_name, &params, skill_service.clone()).await,
        "list_skills" => tool_list_skills(tool_name, &params, skill_service.clone()).await,
        "match_skills" => tool_match_skills(tool_name, &params, skill_service.clone()).await,
        "execute_skill" => tool_execute_skill(tool_name, &params, skill_service.clone()).await,
        "read_knowledge" => tool_read_knowledge(&params).await,
        "write_knowledge" => tool_write_knowledge(&params).await,
        "list_knowledge" => tool_list_knowledge().await,
        "send_to_agent" => tool_send_to_agent(session_id, &params, api_key.clone(), skill_service.clone(), mcp_service.clone(), db.clone(), code_graph.clone(), lsp_manager.clone(), permission_service.clone(), pending_asks.clone(), app_handle.clone()).await,
        "build_code_graph" => tool_build_code_graph(&params, code_graph.clone()).await,
        "code_graph_sync" => tool_code_graph_sync(&params, code_graph.clone()).await,
        "code_graph_search" => tool_code_graph_search(&params, code_graph.clone()).await,
        "code_graph_callers" => tool_code_graph_callers(&params, code_graph.clone()).await,
        "code_graph_callees" => tool_code_graph_callees(&params, code_graph.clone()).await,
        "code_graph_explore" => tool_code_graph_explore(&params, code_graph.clone()).await,
        "code_graph_file" => tool_code_graph_file(&params, code_graph.clone()).await,
        "code_graph_stats" => tool_code_graph_stats(code_graph.clone()).await,
        "read_lints" => tool_read_lints(&params, lsp_manager.clone(), db.clone()).await,
        "touch_file" => tool_touch_file(&params, lsp_manager.clone(), db.clone()).await,
        "ask_choice" => tool_ask_choice(&params, session_id, "unknown", pending_asks.clone(), app_handle.clone()).await,
        // Fallback: try MCP
        _ => {
            let mcp = mcp_service.read().await;
            match mcp.call_tool_any(tool_name, params).await {
                Ok(result) => serde_json::to_string(&result).unwrap_or_else(|e| format!("MCP result serialization failed: {}", e)),
                Err(e) => format!("Tool '{}' not implemented (MCP error: {})", tool_name, e),
            }
        }
    };

    // Auto-touch LSP for file-modifying tools so diagnostics are cached for read_lints
    if let Some(ref fp) = auto_touch_path {
        let lm = lsp_manager;
        let fp_owned = fp.clone();
        let db_clone = db;
        tokio::spawn(async move {
            let touch_params = json!({"file_path": fp_owned});
            let _ = tool_touch_file(&touch_params, lm, db_clone).await;
        });
    }

    result
}

// ========== Agent Communication ==========

async fn tool_send_to_agent(
    caller_session_id: i64,
    params: &serde_json::Value,
    api_key: String,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    db: Arc<StdMutex<Connection>>,
    code_graph: Arc<StdMutex<CodeGraph>>,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    permission_service: Arc<StdMutex<PermissionService>>,
    pending_asks: PendingAsks,
    app_handle: AppHandle,
) -> String {
    let target_agent = params["target_agent"].as_str().unwrap_or("");
    let message = params["message"].as_str().unwrap_or("");

    if target_agent.is_empty() || message.is_empty() {
        return json!({"error": "target_agent and message are required"}).to_string();
    }

    // Look up caller's agent type first
    let caller_agent: String = {
        let conn = db.lock().unwrap();
        conn.query_row(
            "SELECT agent_type FROM agent_session WHERE id = ?1",
            [caller_session_id],
            |row| row.get(0),
        ).unwrap_or_else(|_| "unknown".to_string())
    };
    let tagged_for_db = format!("[来自 {}]\n\n{}", caller_agent, message);

    // DB operations in spawn_blocking
    let db_clone = db.clone();
    let target_agent_owned = target_agent.to_string();

    let db_result = tokio::task::spawn_blocking(move || -> Result<(i64, i64, Vec<Message>), String> {
        let conn = db_clone.lock().unwrap();
        let group_chat_id: i64 = conn
            .query_row(
                "SELECT group_chat_id FROM agent_session WHERE id = ?1",
                rusqlite::params![caller_session_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to find caller session: {}", e))?;

        let target_session_id: i64 = match conn
            .query_row(
                "SELECT id FROM agent_session WHERE group_chat_id = ?1 AND agent_type = ?2",
                rusqlite::params![group_chat_id, target_agent_owned],
                |row| row.get(0),
            )
            .ok()
        {
            Some(id) => id,
            None => {
                conn.execute(
                    "INSERT INTO agent_session (group_chat_id, agent_type) VALUES (?1, ?2)",
                    rusqlite::params![group_chat_id, target_agent_owned],
                )
                .map_err(|e| format!("Failed to create agent session: {}", e))?;
                conn.last_insert_rowid()
            }
        };

        conn.execute(
            "INSERT INTO chat_message (session_id, role, content) VALUES (?1, 'user', ?2)",
            rusqlite::params![target_session_id, tagged_for_db],
        )
        .map_err(|e| format!("Failed to save message: {}", e))?;

        // Load full conversation history for the target agent
        let mut stmt = conn.prepare(
            "SELECT role, content, tool_calls, thinking, raw_json FROM chat_message WHERE session_id = ?1 ORDER BY created_at ASC"
        ).map_err(|e| format!("Failed to prepare history query: {}", e))?;
        let history: Vec<Message> = stmt.query_map(
            rusqlite::params![target_session_id],
            |row| Ok(Message {
                role: row.get(0)?,
                content: row.get(1)?,
                tool_calls: row.get(2)?,
                thinking: row.get(3)?,
                raw_json: row.get(4)?,
            }),
        ).map_err(|e| format!("Failed to query history: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect history: {}", e))?;

        eprintln!("[send_to_agent] Loaded {} history messages for target session {}", history.len(), target_session_id);

        Ok((group_chat_id, target_session_id, history))
    })
    .await;

    match db_result {
        Ok(Ok((group_chat_id, target_session_id, history))) => {
            eprintln!("[send_to_agent] caller_session={}, target={}, target_session={}, group_chat={}, history_len={}",
                caller_session_id, target_agent, target_session_id, group_chat_id, history.len());

            // Emit agent_invoked FIRST so frontend can set up listeners before stream starts
            let _ = app_handle.emit("agent_invoked", json!({
                "target_agent": target_agent,
                "session_id": target_session_id,
                "group_chat_id": group_chat_id,
                "message": message,
            }));

            // Small delay to let frontend listeners attach
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;

            // Spawn the target agent stream in a dedicated thread
            let agent_type = target_agent.to_string();
            let app = app_handle.clone();
            let handle = tokio::runtime::Handle::current();
            let lm = lsp_manager.clone();
            let pm = permission_service.clone();
            let pa = pending_asks.clone();

            // Read workspace from DB so target agent knows the project directory
            let (workspace, api_url, messages_path, model, context_window, provider) = {
                let conn = db.lock().unwrap();
                let ws: Option<String> = conn.query_row("SELECT workspace FROM app_config", [], |row| row.get(0)).ok();
                let provider: String = conn.query_row("SELECT provider FROM app_config", [], |row| row.get(0))
                    .unwrap_or_else(|_| "minimax".to_string());
                match provider.as_str() {
                    "custom" => {
                        let url: String = conn.query_row("SELECT custom_api_url FROM app_config", [], |row| row.get(0)).unwrap_or_default();
                        let m: String = conn.query_row("SELECT custom_model FROM app_config", [], |row| row.get(0)).unwrap_or_default();
                        let cw: i64 = conn.query_row("SELECT custom_context_window FROM app_config", [], |row| row.get(0)).unwrap_or(200000);
                        (ws, url, "/v1/messages".to_string(), m, cw.max(0) as usize, provider)
                    }
                    _ => {
                        let url: String = conn.query_row("SELECT api_url FROM app_config", [], |row| row.get(0))
                            .unwrap_or_else(|_| DEFAULT_API_URL.to_string());
                        let m: String = conn.query_row("SELECT model FROM app_config", [], |row| row.get(0))
                            .unwrap_or_else(|_| "MiniMax-M2.7".to_string());
                        let cw: i64 = conn.query_row("SELECT context_window FROM app_config", [], |row| row.get(0)).unwrap_or(204800);
                        (ws, url, "/anthropic/v1/messages".to_string(), m, cw.max(0) as usize, provider)
                    }
                }
            };

            std::thread::spawn(move || {
                handle.block_on(async move {
                    let agent = AgentService::new(api_key, api_url, messages_path, model, context_window, provider, skill_service, mcp_service, db, code_graph, lm, pm, pa);
                    agent.stream_chat(&agent_type, history, None, workspace, app, target_session_id).await;
                });
            });

            json!({
                "success": true,
                "target_agent": target_agent,
                "session_id": target_session_id,
                "group_chat_id": group_chat_id,
                "message": format!("已向 {} 发送消息", target_agent)
            }).to_string()
        }
        Ok(Err(e)) => json!({"error": e}).to_string(),
        Err(e) => json!({"error": format!("Task panicked: {}", e)}).to_string(),
    }
}

// ========== Tool Implementations (Flat) ==========

async fn tool_list_dir(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut result: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let prefix = if is_dir { "[DIR]" } else { "[FILE]" };
                        format!("{} {}", prefix, e.file_name().to_string_lossy())
                    })
                    .collect();
                result.sort();
                result.join("\n")
            }
            Err(e) => format!("Error: Cannot read directory '{}'", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_read_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let offset = params["offset"].as_u64().unwrap_or(0) as usize;
    let limit = params["limit"].as_u64().unwrap_or(0) as usize;

    tokio::task::spawn_blocking(move || {
        const MAX_SIZE: u64 = 2 * 1024 * 1024; // 2MB
        const OUTLINE_SIZE: u64 = 64 * 1024; // 64KB
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };
        // Binary detection
        if is_binary_file(&path) {
            return format!("Binary file: {} ({} KB). Use a hex editor or specialized tool.", path, meta.len() / 1024);
        }
        // Large file: outline mode
        if meta.len() > MAX_SIZE && offset == 0 && limit == 0 {
            return format!(
                "Large file: {} MB, {} lines. Too big to read entirely.\nUse offset/limit to read a range.\n\nFirst 300 lines for orientation:\n{}",
                meta.len() / 1024 / 1024,
                count_lines(&path),
                read_line_range(&path, 1, 300)
            );
        }
        // Read with offset/limit
        if offset > 0 || limit > 0 {
            let start = if offset > 0 { offset } else { 1 };
            let end = if limit > 0 { start + limit - 1 } else { start + 200 };
            let total = count_lines(&path);
            let prefix = if meta.len() > OUTLINE_SIZE {
                format!("[lines {}-{} of {}]\n", start, std::cmp::min(end, total), total)
            } else {
                String::new()
            };
            return format!("{}{}", prefix, read_line_range(&path, start, end));
        }
        // Normal read
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Error: {}", e))
            .unwrap_or_else(|e| e);
        if meta.len() > OUTLINE_SIZE {
            format!("[{} lines, {} KB]\n{}", count_lines(&path), meta.len() / 1024, content)
        } else {
            content
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn is_binary_file(path: &str) -> bool {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut buf = [0u8; 8192];
        if let Ok(n) = f.read(&mut buf) {
            buf[..n].iter().any(|&b| b == 0)
        } else {
            false
        }
    } else {
        false
    }
}

fn count_lines(path: &str) -> usize {
    use std::io::{BufRead, BufReader};
    if let Ok(f) = std::fs::File::open(path) {
        BufReader::new(f).lines().count()
    } else {
        0
    }
}

fn read_line_range(path: &str, start: usize, end: usize) -> String {
    use std::io::{BufRead, BufReader};
    if let Ok(f) = std::fs::File::open(path) {
        BufReader::new(f).lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let line_num = i + 1;
                if line_num >= start && line_num <= end {
                    l.ok().map(|text| format!("{:>6}| {}", line_num, text))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "Error reading file".to_string()
    }
}

async fn tool_read_files(params: &serde_json::Value) -> String {
    if let Some(paths) = params["paths"].as_array() {
        let results: Vec<String> = paths.iter()
            .filter_map(|p| p.as_str())
            .map(|path| {
                let path = normalize_file_path(path);
                std::fs::read_to_string(&path)
                    .map(|c| format!("=== {} ===\n{}", path, c))
                    .unwrap_or_else(|e| format!("=== {} ===\nError: {}", path, e))
            })
            .collect();
        results.join("\n\n")
    } else {
        "Error: Invalid paths parameter".to_string()
    }
}

async fn tool_git_status(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "status", "--porcelain"])
            .output()
        {
            Ok(o) => {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_log(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let count = params["count"].as_i64().unwrap_or(20) as usize;
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "log", "--oneline", &format!("-{}", count)])
            .output()
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_diff(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let target = params["target"].as_str().unwrap_or("HEAD");
    let path = path.to_string();
    let target = target.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "diff", &target])
            .output()
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_commit(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let message = params["message"].as_str().unwrap_or("");
    let path = path.to_string();
    let message = message.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "commit", "-m", &message])
            .output()
        {
            Ok(o) => {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_branch(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "branch", "-v"])
            .output()
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_checkout(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let branch = params["branch"].as_str().unwrap_or("");
    let path = path.to_string();
    let branch = branch.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "checkout", &branch])
            .output()
        {
            Ok(o) => {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_stash(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "stash", "push", "-m", "auto-stash"])
            .output()
        {
            Ok(o) => {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_git_stash_pop(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("git")
            .args(["-C", &path, "stash", "pop"])
            .output()
        {
            Ok(o) => {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_search_in_dir(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let pattern = params["pattern"].as_str().unwrap_or("").to_lowercase();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut matches = Vec::new();
        search_recursive(&path, &pattern, 0, 10, &mut matches);
        if matches.is_empty() {
            "No matches found".to_string()
        } else {
            matches.iter()
                .take(10)
                .map(|(file, line_num, line)| format!("{}:{}: {}", file, line_num, line))
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_get_env_info(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || get_env_info_sync(&path))
        .await
        .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_analyze_project_structure(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || analyze_project_sync(&path))
        .await
        .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_run_command(params: &serde_json::Value) -> String {
    let command = params["command"].as_str().unwrap_or("").to_string();
    let cwd = params["path"].as_str().map(|s| normalize_file_path(s));
    tokio::task::spawn_blocking(move || {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", &command]);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &command]);
            c
        };
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        match cmd.output() {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                let exit = o.status.code().unwrap_or(-1);
                format!("Exit: {}\n{}\n{}", exit, stdout, stderr)
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_write_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let content = params["content"].as_str().unwrap_or("").to_string();
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return format!("Error creating directory: {}", e);
            }
        }
        std::fs::write(&path, &content)
            .map_err(|e| format!("Error: {}", e))
            .map(|_| format!("Written to {}", path))
            .unwrap_or_else(|e| e)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_write_files(params: &serde_json::Value) -> String {
    if let Some(files) = params["files"].as_array() {
        let results: Vec<String> = files.iter()
            .filter_map(|f| f.as_object())
            .map(|obj| {
                let path = normalize_file_path(obj.get("path").and_then(|p| p.as_str()).unwrap_or(""));
                let content = obj.get("content").and_then(|c| c.as_str()).unwrap_or("");
                if path.is_empty() {
                    return "Error: empty path".to_string();
                }
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&path, content) {
                    Ok(_) => format!("Written: {}", path),
                    Err(e) => format!("Error writing {}: {}", path, e),
                }
            })
            .collect();
        results.join("\n")
    } else {
        "Error: Invalid files parameter".to_string()
    }
}

async fn tool_find_replace_in_files(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let find = params["find"].as_str().unwrap_or("");
    let replace = params["replace"].as_str().unwrap_or("");
    let use_regex = params.get("use_regex").and_then(|b| b.as_bool()).unwrap_or(false);
    let path = path.to_string();
    let find = find.to_string();
    let replace = replace.to_string();
    tokio::task::spawn_blocking(move || {
        let mut count = 0;
        find_replace_recursive(&path, &find, &replace, use_regex, &mut count, 0, 10);
        format!("Modified {} files", count)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_modify_files(params: &serde_json::Value) -> String {
    if let Some(files) = params["files"].as_array() {
        let results: Vec<String> = files.iter()
            .filter_map(|f| f.as_object())
            .map(|obj| {
                let path = normalize_file_path(obj.get("path").and_then(|p| p.as_str()).unwrap_or(""));
                if path.is_empty() {
                    return "Error: empty path".to_string();
                }
                let original = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => return format!("Error reading {}: {}", path, e),
                };
                let mut content = original;
                if let Some(replacements) = obj.get("replacements").and_then(|r| r.as_array()) {
                    for rep in replacements.iter().filter_map(|r| r.as_object()) {
                        let find = rep.get("find").and_then(|f| f.as_str()).unwrap_or("");
                        let replace_with = rep.get("replace").and_then(|r| r.as_str()).unwrap_or("");
                        content = content.replace(find, replace_with);
                    }
                }
                if let Some(new_content) = obj.get("new_content").and_then(|c| c.as_str()) {
                    content = new_content.to_string();
                }
                match std::fs::write(&path, &content) {
                    Ok(_) => format!("Modified: {}", path),
                    Err(e) => format!("Error modifying {}: {}", path, e),
                }
            })
            .collect();
        results.join("\n")
    } else {
        "Error: Invalid files parameter".to_string()
    }
}

async fn tool_get_file_info(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let is_dir = meta.is_dir();
                let size = meta.len();
                let modified = meta.modified()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|_| "unknown".to_string());
                format!("Type: {}, Size: {} bytes, Modified: {}",
                    if is_dir { "directory" } else { "file" }, size, modified)
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_directory_tree(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let max_depth = params["max_depth"].as_i64().unwrap_or(2) as usize;
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut output = Vec::new();
        let root = std::path::Path::new(&path);
        if let Some(name) = root.file_name() {
            output.push(name.to_string_lossy().to_string());
        }
        tree_recursive(root, "", max_depth, &mut output);
        output.join("\n")
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn tree_recursive(dir: &std::path::Path, prefix: &str, max_depth: usize, output: &mut Vec<String>) {
    if max_depth == 0 { return; }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if a_dir && !b_dir { std::cmp::Ordering::Less }
        else if !a_dir && b_dir { std::cmp::Ordering::Greater }
        else { a.file_name().cmp(&b.file_name()) }
    });
    let skip = |name: &str| name.starts_with('.') || matches!(name, "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__");
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name().to_string_lossy().to_string();
        if skip(&name) { continue; }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let is_last = i == entries.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };
        output.push(format!("{}{}{}{}", prefix, branch, name, if is_dir { "/" } else { "" }));
        if is_dir {
            let path = entry.path();
            let new_prefix = format!("{}{}", prefix, child_prefix);
            tree_recursive(&path, &new_prefix, max_depth - 1, output);
        }
    }
}

async fn tool_glob(params: &serde_json::Value) -> String {
    let pattern = params["pattern"].as_str().unwrap_or("*").to_string();
    let base_path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let limit = params["limit"].as_i64().unwrap_or(200) as usize;
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        let skip_dirs = |name: &str| name.starts_with('.') || matches!(name, "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__");
        glob_recursive(&base_path, &pattern, &skip_dirs, &mut results, limit);
        results.sort_by(|a, b| {
            let ma = std::fs::metadata(a).and_then(|m| m.modified()).ok();
            let mb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
            mb.cmp(&ma)
        });
        results.iter().map(|p| p.strip_prefix(&base_path).unwrap_or(p).to_string()).collect::<Vec<_>>().join("\n")
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn glob_recursive(dir: &str, pattern: &str, skip_dirs: &dyn Fn(&str) -> bool, results: &mut Vec<String>, limit: usize) {
    if results.len() >= limit { return; }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };
    for entry in entries {
        if results.len() >= limit { break; }
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if skip_dirs(&name) { continue; }
            let path_str = entry.path().to_string_lossy().to_string();
            glob_recursive(&path_str, pattern, skip_dirs, results, limit);
        } else {
            let matches = if pattern.contains('*') || pattern.contains('?') {
                let re = glob_to_regex(&pattern);
                regex::Regex::new(&re).map(|r| r.is_match(&name)).unwrap_or(false)
            } else {
                name.to_lowercase().contains(&pattern.to_lowercase())
            };
            if matches {
                results.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
}

fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(ch);
            }
            _ => re.push(ch),
        }
    }
    re.push('$');
    re
}

async fn tool_search_files(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let pattern = params["pattern"].as_str().unwrap_or("");
    let path = path.to_string();
    let pattern = pattern.to_lowercase();
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        search_files_recursive(&path, &pattern, &mut results, 100);
        if results.is_empty() { "No files found".to_string() } else { results.join("\n") }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn search_files_recursive(dir: &str, pattern: &str, results: &mut Vec<String>, limit: usize) {
    if results.len() >= limit { return; }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };
    for entry in entries {
        if results.len() >= limit { break; }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | ".git" | "dist" | "build") { continue; }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let path_str = entry.path().to_string_lossy().to_string();
        if is_dir {
            search_files_recursive(&path_str, pattern, results, limit);
        } else if name.to_lowercase().contains(pattern) {
            results.push(path_str);
        }
    }
}

async fn tool_edit_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let search = params["search"].as_str().unwrap_or("").to_string();
    let replace = params["replace"].as_str().unwrap_or("").to_string();

    if search.is_empty() {
        return "Error: search cannot be empty".to_string();
    }

    tokio::task::spawn_blocking(move || {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };

        // Auto-detect line endings and normalize search/replace to match
        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let search = search.replace("\r\n", "\n").replace('\n', line_ending);
        let replace = replace.replace("\r\n", "\n").replace('\n', line_ending);

        let count = content.matches(&search).count();
        if count == 0 {
            return format!(
                "Error: 未找到要替换的文本。文件可能已被修改，请先 read_file 确认当前内容。\n文件: {}\n查找: {}",
                path, truncate_str(&search, 200)
            );
        }
        if count > 1 {
            return format!(
                "Error: 找到 {} 处匹配，不够唯一。请增加更多上下文（前后各 3-5 行）使匹配唯一。", count
            );
        }

        let new_content = content.replacen(&search, &replace, 1);

        match std::fs::write(&path, &new_content) {
            Ok(_) => {
                let diff = compute_diff(&content, &new_content);
                format!("edited {} ({})\n{}", path, count_chars(&content, &new_content), diff)
            }
            Err(e) => format!("Error writing {}: {}", path, e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn count_chars(old: &str, new: &str) -> String {
    let old_len = old.len();
    let new_len = new.len();
    if new_len >= old_len {
        format!("{}->{} chars, +{}", old_len, new_len, new_len - old_len)
    } else {
        format!("{}->{} chars, -{}", old_len, new_len, old_len - new_len)
    }
}

fn compute_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = String::new();
    // Find changed region: first diff line to last diff line
    let mut first_diff = 0;
    while first_diff < old_lines.len() && first_diff < new_lines.len()
        && old_lines[first_diff] == new_lines[first_diff] {
        first_diff += 1;
    }
    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > first_diff && new_end > first_diff
        && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    // Show context (1 line before)
    let ctx_start = if first_diff > 0 { first_diff - 1 } else { 0 };
    result.push_str(&format!("@@ -{},{} +{},{} @@\n",
        ctx_start + 1, old_end - ctx_start, ctx_start + 1, new_end - ctx_start));
    for i in ctx_start..old_end {
        if i < old_lines.len() {
            result.push_str(&format!("-{}\n", old_lines[i]));
        }
    }
    for i in ctx_start..new_end {
        if i < new_lines.len() {
            result.push_str(&format!("+{}\n", new_lines[i]));
        }
    }
    result
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

async fn tool_edit_lines(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    let start_line = params["start_line"].as_u64().unwrap_or(0) as usize;
    let end_line = params["end_line"].as_u64().map(|v| v as usize);
    let content = params["content"].as_str().map(|s| s.to_string());

    if path.is_empty() || start_line == 0 {
        return "Error: path and start_line are required".to_string();
    }

    tokio::task::spawn_blocking(move || {
        let file_content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading {}: {}", path, e),
        };
        let lines: Vec<&str> = file_content.lines().collect();
        let total = lines.len();

        if start_line > total + 1 {
            return format!("Error: start_line {} out of range (file has {} lines)", start_line, total);
        }

        let end = end_line.unwrap_or(start_line).min(total);
        let old_lines: Vec<String> = lines[start_line - 1..end].iter().map(|l| l.to_string()).collect();

        match (content.as_ref(), end_line) {
            (Some(text), Some(_)) => {
                // Replace: remove lines [start_line, end_line], insert content
                let new_lines: Vec<&str> = text.lines().collect();
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(new_lines.iter().map(|l| l.to_string()));
                result.extend(lines[end..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: replaced lines {}-{} with {} lines\n-{}\n+{}",
                        path, start_line, end, new_lines.len(),
                        old_lines.join("\n-"),
                        text.lines().collect::<Vec<_>>().join("\n+")),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (Some(text), None) => {
                // Insert: insert content before start_line
                let new_lines: Vec<&str> = text.lines().collect();
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(new_lines.iter().map(|l| l.to_string()));
                result.extend(lines[start_line - 1..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: inserted {} lines at line {}", path, new_lines.len(), start_line),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (None, Some(_)) => {
                // Delete: remove lines [start_line, end_line]
                let mut result: Vec<String> = lines[..start_line - 1].iter().map(|l| l.to_string()).collect();
                result.extend(lines[end..].iter().map(|l| l.to_string()));
                let new_content = result.join("\n");
                match std::fs::write(&path, &new_content) {
                    Ok(_) => format!("edit_lines {}: deleted lines {}-{}\n-{}",
                        path, start_line, end,
                        old_lines.join("\n-")),
                    Err(e) => format!("Error: {}", e),
                }
            }
            (None, None) => {
                format!("Error: either end_line or content must be provided")
            }
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_multi_edit(params: &serde_json::Value) -> String {
    let edits: Vec<(String, String, String)> = match params["edits"].as_array() {
        Some(arr) => arr.iter().map(|e| (
            e["path"].as_str().unwrap_or("").to_string(),
            e["search"].as_str().unwrap_or("").to_string(),
            e["replace"].as_str().unwrap_or("").to_string(),
        )).collect(),
        None => return r#"{"error": "edits array is required"}"#.to_string(),
    };

    tokio::task::spawn_blocking(move || {
        // Phase 1: Validate all edits
        let mut validated: Vec<(String, String, String, String)> = Vec::new();
        for (i, (path, search, replace)) in edits.iter().enumerate() {
            let path = path.clone();
            let search = search.clone();
            let replace = replace.clone();
            if path.is_empty() || search.is_empty() {
                return format!("Error: edit #{} has empty path or search", i + 1);
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return format!("Error reading {}: {}", path, e),
            };
            let count = content.matches(&search).count();
            if count == 0 {
                return format!(
                    "multi_edit: edit #{} failed — search text not found in {}. No files were modified.",
                    i + 1, path
                );
            }
            if count > 1 {
                return format!(
                    "multi_edit: edit #{} failed — search appears {} times in {}. Include more context. No files modified.",
                    i + 1, count, path
                );
            }
            validated.push((path, search, replace, content));
        }

        // Phase 2: Apply all edits
        let mut results = Vec::new();
        let mut original_contents: Vec<(String, String)> = Vec::new();
        for (path, search, replace, _) in &validated {
            let content = std::fs::read_to_string(path)
                .unwrap_or_default();
            original_contents.push((path.clone(), content.clone()));
            let new_content = content.replacen(search, replace, 1);
            match std::fs::write(path, &new_content) {
                Ok(_) => results.push(format!("{}: applied", path)),
                Err(e) => {
                    // Rollback
                    for (rb_path, rb_content) in &original_contents {
                        let _ = std::fs::write(rb_path, rb_content);
                    }
                    return format!("multi_edit: write failed for {}: {}. All files rolled back.", path, e);
                }
            }
        }
        format!("multi_edit: {} edits applied across {} files\n{}",
            validated.len(),
            validated.iter().map(|(p,_,_,_)| p).collect::<std::collections::HashSet<_>>().len(),
            results.join("\n"))
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_create_directory(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        match std::fs::create_dir_all(&path) {
            Ok(_) => format!("Created directory: {}", path),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_move_file(params: &serde_json::Value) -> String {
    let source = normalize_file_path(params["source"].as_str().unwrap_or(""));
    let destination = normalize_file_path(params["destination"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        match std::fs::rename(&source, &destination) {
            Ok(_) => format!("Moved: {} -> {}", source, destination),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_delete_file(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.is_dir() {
                return "Error: is a directory. Use a different tool to delete directories.".to_string();
            }
        }
        match std::fs::remove_file(&path) {
            Ok(_) => format!("Deleted: {}", path),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_copy_file_fn(params: &serde_json::Value) -> String {
    let source = normalize_file_path(params["source"].as_str().unwrap_or(""));
    let destination = normalize_file_path(params["destination"].as_str().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        copy_recursive(&source, &destination)
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn copy_recursive(src: &str, dst: &str) -> String {
    let src_path = std::path::Path::new(src);
    if !src_path.exists() {
        return format!("Error: source '{}' does not exist", src);
    }
    if src_path.is_dir() {
        match std::fs::create_dir_all(dst) {
            Ok(_) => {},
            Err(e) => return format!("Error: {}", e),
        }
        let entries = match std::fs::read_dir(src) {
            Ok(e) => e,
            Err(e) => return format!("Error: {}", e),
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let sub_src = src_path.join(&name);
            let sub_dst = std::path::Path::new(dst).join(&name);
            let result = copy_recursive(&sub_src.to_string_lossy(), &sub_dst.to_string_lossy());
            if result.starts_with("Error") { return result; }
        }
        format!("Copied directory: {} -> {}", src, dst)
    } else {
        match std::fs::copy(src, dst) {
            Ok(_) => format!("Copied: {} -> {}", src, dst),
            Err(e) => format!("Error: {}", e),
        }
    }
}

async fn tool_web_fetch(params: serde_json::Value) -> String {
    let url = params["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return "Error: url is required".to_string();
    }
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        match reqwest::blocking::get(&url) {
            Ok(resp) => {
                match resp.text() {
                    Ok(html) => {
                        let text = html_to_text(&html);
                        if text.len() > 32000 {
                            format!("{}...\n[truncated at 32K chars]", &text[..32000])
                        } else {
                            text
                        }
                    }
                    Err(e) => format!("Error reading response: {}", e),
                }
            }
            Err(e) => format!("Error fetching URL: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

fn html_to_text(html: &str) -> String {
    // Strip script and style tags
    let mut text = html.to_string();
    while let Some(start) = text.find("<script") {
        if let Some(end) = text[start..].find("</script>") {
            text.replace_range(start..start + end + 9, " ");
        } else { break; }
    }
    while let Some(start) = text.find("<style") {
        if let Some(end) = text[start..].find("</style>") {
            text.replace_range(start..start + end + 8, " ");
        } else { break; }
    }
    // Strip all HTML tags
    let mut result = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Collapse whitespace
    let lines: Vec<&str> = result.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

async fn tool_run_background(params: &serde_json::Value) -> String {
    let command = params["command"].as_str().unwrap_or("");
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let wait_sec = params["wait_sec"].as_i64().unwrap_or(3) as u64;
    if command.is_empty() {
        return "Error: command is required".to_string();
    }
    eprintln!("[run_background] Spawning: {} (cwd: {})", command, path);

    // Temp dir for output capture
    let tmp_dir = std::path::PathBuf::from(
        std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
    ).join(".minimaxcode").join("tmp");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let out_file = tmp_dir.join(format!("bg_out_{}.txt", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
    let err_file = tmp_dir.join(format!("bg_err_{}.txt", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() + 1));

    let out_file_clone = out_file.clone();
    let err_file_clone = err_file.clone();

    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = std::process::Command::new("bash");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(path);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    match cmd.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            let child_stdout = child.stdout.take();
            let child_stderr = child.stderr.take();

            // Spawn threads to capture output to files in real-time
            if let Some(stdout) = child_stdout {
                let out_path = out_file_clone.clone();
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader, Write};
                    let reader = BufReader::new(stdout);
                    let mut file = std::fs::File::create(&out_path).unwrap();
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            let _ = writeln!(file, "{}", line);
                            let _ = file.flush();
                        }
                    }
                });
            }
            if let Some(stderr) = child_stderr {
                let err_path = err_file_clone;
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader, Write};
                    let reader = BufReader::new(stderr);
                    let mut file = std::fs::File::create(&err_path).unwrap();
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            let _ = writeln!(file, "{}", line);
                            let _ = file.flush();
                        }
                    }
                });
            }

            // Small wait for initial output
            std::thread::sleep(std::time::Duration::from_secs(wait_sec));
            let startup = std::fs::read_to_string(&out_file_clone).unwrap_or_default();

            // Spawn reaper to write exit status when process ends
            let out_reap = out_file_clone.clone();
            std::thread::spawn(move || {
                let status = child.wait(); // blocks until process exits
                if let Ok(ref f) = std::fs::OpenOptions::new().append(true).open(&out_reap) {
                    use std::io::Write;
                    let mut f_ref = f;
                    let _ = writeln!(f_ref, "\n--- 进程退出码: {:?} ---", status.as_ref().ok().and_then(|s| s.code()));
                }
            });

            json!({
                "success": true,
                "pid": pid,
                "out_file": out_file.to_string_lossy().to_string(),
                "err_file": err_file.to_string_lossy().to_string(),
                "startup_output": startup,
                "message": format!("后台进程已启动，PID: {}, 输出文件: {}", pid, out_file.to_string_lossy())
            }).to_string()
        }
        Err(e) => json!({"error": format!("Failed to spawn: {}", e)}).to_string(),
    }
}

async fn tool_job_output(params: &serde_json::Value) -> String {
    let pid = params["job_id"].as_i64().unwrap_or(0) as u32;
    let tail = params["tail_lines"].as_i64().unwrap_or(200) as usize;
    // Allow reading output from file directly (preferred over PID for live output)
    let out_file = params["out_file"].as_str().unwrap_or("");

    if !out_file.is_empty() && std::path::Path::new(out_file).exists() {
        let content = match std::fs::read_to_string(out_file) {
            Ok(c) => c,
            Err(e) => return format!("读取输出文件失败: {}", e),
        };
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let tail_start = if total > tail { total - tail } else { 0 };
        let tail_text: String = lines[tail_start..].iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        // Check if process is still running
        let status = if cfg!(windows) {
            std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        } else {
            std::process::Command::new("ps")
                .args(["-p", &pid.to_string()])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        };

        return format!("{} ({} / {} lines)\n{}",
            if status { "运行中" } else { "已结束" },
            tail_text.lines().count(),
            total,
            tail_text.trim());
    }

    if pid == 0 {
        return "Error: 需要 job_id 或 out_file 参数".to_string();
    }
    // Legacy: try to get process info via tasklist/ps
    if cfg!(windows) {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output();
        match output {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.contains(&pid.to_string()) {
                    format!("Process {} is running.\n{}", pid, text)
                } else {
                    format!("Process {} is not running. 如果之前返回了 out_file，请用 out_file 参数读取输出。", pid)
                }
            }
            Err(_) => format!("Process {} status unknown.", pid),
        }
    } else {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid,stat,command"])
            .output();
        match output {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.contains(&pid.to_string()) {
                    format!("Process {} is running:\n{}", pid, text)
                } else {
                    format!("Process {} is not running. 如果之前返回了 out_file，请用 out_file 参数读取输出。", pid)
                }
            }
            Err(_) => format!("Process {} status unknown.", pid),
        }
    }
}

async fn tool_list_jobs(_params: &serde_json::Value) -> String {
    if cfg!(windows) {
        match std::process::Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()
        {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.trim().is_empty() {
                    "No running processes detected.".to_string()
                } else {
                    format!("Running processes:\n{}", text.lines().take(50).collect::<Vec<_>>().join("\n"))
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    } else {
        match std::process::Command::new("ps").args(["-eo", "pid,comm,stat"]).output() {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                if text.trim().is_empty() {
                    "No running processes detected.".to_string()
                } else {
                    format!("Running processes:\n{}", text.lines().take(50).collect::<Vec<_>>().join("\n"))
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

async fn tool_run_tests(params: &serde_json::Value) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let framework = params["test_framework"].as_str().unwrap_or("npm");
    let path = path.to_string();
    let framework = framework.to_string();
    tokio::task::spawn_blocking(move || {
        let (cmd, args) = match framework.as_str() {
            "jest" => ("npm", vec!["test", "--", "--coverage=false"]),
            "pytest" => ("python", vec!["-m", "pytest", "--tb=short", "-q"]),
            "cargo" => ("cargo", vec!["test", "--", "--nocapture"]),
            "npm" => ("npm", vec!["test", "--", "--coverage=false"]),
            _ => return format!("Unknown test framework: {}", framework),
        };
        let mut process = std::process::Command::new(cmd);
        if framework == "pytest" || framework == "cargo" {
            process.current_dir(&path);
        }
        process.args(&args);
        match process.output() {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                if stdout.is_empty() { stderr } else { stdout }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_spawn_process(params: &serde_json::Value) -> String {
    let command = params["command"].as_str().unwrap_or("");
    let cwd = params.get("path").and_then(|p| p.as_str());
    let command = command.to_string();
    let cwd = cwd.map(|s| s.to_string());
    tokio::task::spawn_blocking(move || {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", "/B", &command]);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &format!("{} &", command)]);
            c
        };
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        match cmd.spawn() {
            Ok(child) => format!("Process started with PID: {}", child.id()),
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

async fn tool_kill_process(params: &serde_json::Value) -> String {
    let pid = params["pid"].as_i64().unwrap_or(0) as u32;
    tokio::task::spawn_blocking(move || {
        let output = if cfg!(windows) {
            std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output()
        } else {
            std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output()
        };
        match output {
            Ok(o) => {
                if o.status.success() {
                    "Process killed".to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    })
    .await
    .unwrap_or_else(|_| "Task cancelled".to_string())
}

/// Vision analysis using Anthropic Messages API for custom providers.
async fn vision_anthropic(prompt: &str, image_url: &str, api_key: &str, api_url: &str, model: &str) -> String {
    let (mime, base64_data) = match resolve_image_base64(image_url) {
        Ok((m, b)) => (m, b),
        Err(e) => return format!(r#"{{"success": false, "error": "Failed to load image: {}"}}"#, e),
    };

    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": 2048,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime,
                        "data": base64_data
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }]
    });

    let client = reqwest::Client::new();
    let messages_path = format!("{}/v1/messages", api_url.trim_end_matches('/'));
    match client.post(&messages_path)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
    {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    let text = data["content"].as_array()
                        .and_then(|arr| arr.iter().find(|b| b["type"] == "text"))
                        .and_then(|b| b["text"].as_str())
                        .unwrap_or("");
                    serde_json::to_string(&serde_json::json!({
                        "success": true,
                        "content": text
                    })).unwrap_or_default()
                }
                Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
            }
        }
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

fn resolve_image_base64(image_url: &str) -> Result<(String, String), String> {
    let data = if image_url.starts_with("http://") || image_url.starts_with("https://") {
        reqwest::blocking::get(image_url)
            .map_err(|e| e.to_string())?
            .bytes()
            .map_err(|e| e.to_string())?
            .to_vec()
    } else {
        std::fs::read(image_url).map_err(|e| format!("Cannot read {}: {}", image_url, e))?
    };
    let mime = if data.len() >= 3 && &data[0..3] == b"\xFF\xD8\xFF" { "image/jpeg" }
        else if data.len() >= 4 && &data[0..4] == b"\x89PNG" { "image/png" }
        else if data.len() >= 4 && &data[0..4] == b"RIFF" { "image/webp" }
        else { "image/png" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok((mime.to_string(), b64))
}

async fn tool_web_search(params: serde_json::Value, api_key: String, api_url: String, provider: String) -> String {
    let query = params["query"].as_str().unwrap_or("").to_string();
    if query.is_empty() {
        return r#"{"success": false, "error": "query is required"}"#.to_string();
    }

    // Custom providers don't have web_search registered (agent uses MCP tool instead).
    // If we reach here with a custom provider, something went wrong — guide agent to MCP.
    if provider == "custom" {
        return r#"{"success": false, "error": "Web search is not available for this provider. Connect an MCP search tool to enable it."}"#.to_string();
    }

    // MiniMax provider: use MiniMax's built-in search API
    let search_url = format!("{}/v1/coding_plan/search", api_url);
    tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        let resp = client.post(&search_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({ "q": query }))
            .timeout(std::time::Duration::from_secs(SEARCH_TIMEOUT_SECS))
            .send();

        match resp {
            Ok(response) => {
                match response.json::<serde_json::Value>() {
                    Ok(data) => {
                        let results = data.get("organic")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter().take(10).map(|item| {
                                    serde_json::json!({
                                        "title": item.get("title").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "link": item.get("link").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "snippet": item.get("snippet").unwrap_or(&serde_json::Value::String("".to_string())),
                                        "date": item.get("date").unwrap_or(&serde_json::Value::String("".to_string()))
                                    })
                                }).collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        serde_json::to_string(&serde_json::json!({
                            "success": true,
                            "results": results,
                            "related_searches": data.get("related_searches").cloned().unwrap_or(serde_json::Value::Array(vec![]))
                        })).unwrap_or_else(|_| r#"{"success": false, "error": "JSON serialization failed"}"#.to_string())
                    }
                    Err(e) => serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": format!("Failed to parse response: {}", e)
                    })).unwrap()
                }
            }
            Err(e) => serde_json::to_string(&serde_json::json!({
                "success": false,
                "error": format!("Request failed: {}", e)
            })).unwrap()
        }
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

async fn tool_understand_image(params: serde_json::Value, api_key: String, api_url: String, model: String, provider: String) -> String {
    let prompt = params["prompt"].as_str().unwrap_or("").to_string();
    let image_url = params["image_url"].as_str().unwrap_or("").to_string();

    if prompt.is_empty() || image_url.is_empty() {
        return r#"{"success": false, "error": "prompt and image_url are required"}"#.to_string();
    }

    if provider == "custom" {
        return vision_anthropic(&prompt, &image_url, &api_key, &api_url, &model).await;
    }

    // MiniMax provider: use MiniMax vision API
    tokio::task::spawn_blocking(move || {
        let image_data = resolve_image(&image_url);

        let client = reqwest::blocking::Client::new();
        let resp = client.post(format!("{}/v1/coding_plan/vlm", DEFAULT_API_URL))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "prompt": prompt,
                "image_url": image_data
            }))
            .timeout(std::time::Duration::from_secs(VLM_TIMEOUT_SECS))
            .send();

        match resp {
            Ok(response) => {
                match response.json::<serde_json::Value>() {
                    Ok(data) => {
                        let description = data.get("content")
                            .and_then(|c| c.as_str())
                            .unwrap_or("No description available");
                        serde_json::to_string(&serde_json::json!({
                            "success": true,
                            "description": description
                        })).unwrap_or_else(|_| r#"{"success": false, "error": "JSON serialization failed"}"#.to_string())
                    }
                    Err(e) => serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": format!("Failed to parse response: {}", e)
                    })).unwrap()
                }
            }
            Err(e) => serde_json::to_string(&serde_json::json!({
                "success": false,
                "error": format!("Request failed: {}", e)
            })).unwrap()
        }
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

async fn tool_skill(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let name = params["name"].as_str().unwrap_or("");
    if name.is_empty() {
        return r#"{"success": false, "error": "Skill name is required"}"#.to_string();
    }

    match skill_service.get_skill_content(name).await {
        Some(content) => {
            let skill = skill_service.get_skill(name).await;
            format!(r#"{{"success": true, "skill": {{"name": "{}", "content": {}, "allowed_tools": {:?}}}, "scripts": {:?}, "references": {:?}}}"#,
                name,
                serde_json::to_string(&content).unwrap_or_default(),
                skill.as_ref().map(|s| s.allowed_tools.clone()).unwrap_or_default(),
                skill.as_ref().map(|s| s.scripts.clone()).unwrap_or_default(),
                skill.as_ref().map(|s| s.references.clone()).unwrap_or_default())
        }
        None => format!(r#"{{"success": false, "error": "Skill '{}' not found"}}"#, name),
    }
}

async fn tool_list_skills(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let source = params.get("source").and_then(|s| s.as_str());
    let skills = skill_service.list_skills(source).await;
    serde_json::to_string(&skills).unwrap_or_else(|_| "[]".to_string())
}

async fn tool_match_skills(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let query = params["query"].as_str().unwrap_or("");
    let top_k = params.get("top_k").and_then(|k| k.as_u64()).unwrap_or(3) as usize;
    let matches = skill_service.match_skills(query, top_k).await;
    serde_json::to_string(&matches).unwrap_or_else(|_| "[]".to_string())
}

async fn tool_execute_skill(_tool_name: &str, params: &serde_json::Value, skill_service: Arc<SkillService>) -> String {
    let name = params["name"].as_str().unwrap_or("");
    let script = params.get("script").and_then(|s| s.as_str());
    if name.is_empty() {
        return r#"{"success": false, "error": "Skill name is required"}"#.to_string();
    }
    match skill_service.execute_skill(name, script).await {
        Ok(output) => format!(r#"{{"success": true, "output": {}}}"#, serde_json::to_string(&output).unwrap_or_default()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

async fn tool_read_knowledge(params: &serde_json::Value) -> String {
    let file_name = params["file_name"].as_str().unwrap_or("");

    if file_name.is_empty() {
        return r#"{"success": false, "error": "file_name is required"}"#.to_string();
    }

    let base = user_home_dir();
    // Read workspace from DB to derive project name
    let project_name = get_project_name();
    let path = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge").join(file_name);

    match std::fs::read_to_string(&path) {
        Ok(content) => format!(r#"{{"success": true, "content": {}}}"#, serde_json::to_string(&content).unwrap_or_default()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

async fn tool_write_knowledge(params: &serde_json::Value) -> String {
    let file_name = params["file_name"].as_str().unwrap_or("");
    let content = params["content"].as_str().unwrap_or("");

    if file_name.is_empty() {
        return r#"{"success": false, "error": "file_name is required"}"#.to_string();
    }

    let base = user_home_dir();
    let project_name = get_project_name();
    let dir = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge");
    let path = dir.join(file_name);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&path, content) {
        Ok(_) => format!(r#"{{"success": true, "path": "{}"}}"#, path.display()),
        Err(e) => format!(r#"{{"success": false, "error": "{}"}}"#, e),
    }
}

async fn tool_list_knowledge() -> String {
    tokio::task::spawn_blocking(|| {
        let base = user_home_dir();
        let project_name = get_project_name();
        let dir = base.join(".minimaxcode").join("project mem").join(&project_name).join("knowledge");

        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = meta.len();
                        let modified = meta.modified()
                            .map(|t| format!("{:?}", t))
                            .unwrap_or_default();
                        files.push(serde_json::json!({
                            "name": name,
                            "size": size,
                            "modified": modified
                        }));
                    }
                }
            }
        }
        serde_json::to_string(&serde_json::json!({
            "success": true,
            "files": files
        })).unwrap_or_else(|_| r#"{"success": false, "error": "serialization error"}"#.to_string())
    })
    .await
    .unwrap_or_else(|_| r#"{"success": false, "error": "Task cancelled"}"#.to_string())
}

// ========== Helper Functions ==========

fn search_recursive(path: &str, pattern: &str, depth: usize, max_depth: usize, results: &mut Vec<(String, i32, String)>) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" {
            continue;
        }

        if file_path.is_dir() {
            search_recursive(&file_path.to_string_lossy(), pattern, depth + 1, max_depth, results);
        } else if file_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                for (i, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(pattern) {
                        results.push((
                            file_path.to_string_lossy().to_string(),
                            (i + 1) as i32,
                            line.trim().to_string(),
                        ));
                        if results.len() >= 50 {
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn find_replace_recursive(path: &str, find: &str, replace: &str, use_regex: bool, count: &mut usize, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" {
            continue;
        }

        if file_path.is_dir() {
            find_replace_recursive(&file_path.to_string_lossy(), find, replace, use_regex, count, depth + 1, max_depth);
        } else if file_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let new_content = if use_regex {
                    match regex::Regex::new(find) {
                        Ok(re) => re.replace_all(&content, replace).to_string(),
                        Err(_) => content.replace(find, replace),
                    }
                } else {
                    content.replace(find, replace)
                };
                if new_content != content {
                    if std::fs::write(&file_path, &new_content).is_ok() {
                        *count += 1;
                    }
                }
            }
        }
    }
}

fn get_env_info_sync(repo_path: &str) -> String {
    let mut info = Vec::new();
    let proj_path = std::path::Path::new(repo_path);

    if proj_path.join("package.json").exists() {
        info.push("Project: Node.js/npm".to_string());
    }
    if proj_path.join("Cargo.toml").exists() {
        info.push("Project: Rust/Cargo".to_string());
    }
    if proj_path.join("requirements.txt").exists() || proj_path.join("pyproject.toml").exists() {
        info.push("Project: Python".to_string());
    }

    let output = std::process::Command::new("git")
        .args(["-C", repo_path, "status", "--porcelain"])
        .output();
    if let Ok(o) = output {
        let lines = String::from_utf8_lossy(&o.stdout);
        if lines.trim().is_empty() {
            info.push("Git: Clean".to_string());
        } else {
            info.push("Git: Modified".to_string());
        }
    }

    info.join("\n")
}

fn analyze_project_sync(repo_path: &str) -> String {
    let path = std::path::Path::new(repo_path);
    let mut info = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                info.push(format!("[DIR] {}", name));
            } else {
                info.push(format!("[FILE] {}", name));
            }
        }
    }

    info.join("\n")
}

// ========== Tool Definitions ==========

fn resolve_image(image_url: &str) -> String {
    // Already a data URI — pass through
    if image_url.starts_with("data:") {
        return image_url.to_string();
    }

    // Local file path — read and encode
    let path = std::path::Path::new(image_url);
    if path.exists() {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.len() > 50 * 1024 * 1024 {
                return String::new(); // Too large (>50MB)
            }
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png")
                .to_lowercase();
            let mime = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                _ => "image/png",
            };
            return format!("data:{};base64,{}", mime, base64_encode(&bytes));
        }
    }

    // HTTP URL — fetch and encode
    if image_url.starts_with("http://") || image_url.starts_with("https://") {
        if let Ok(resp) = reqwest::blocking::get(image_url) {
            if let Ok(bytes) = resp.bytes() {
                if bytes.len() > 50 * 1024 * 1024 {
                    return String::new();
                }
                let mime = "image/jpeg"; // default
                return format!("data:{};base64,{}", mime, base64_encode(&bytes));
            }
        }
    }

    // Fallback: pass as-is
    image_url.to_string()
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let val = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((val >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((val >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((val >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(val & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn user_home_dir() -> std::path::PathBuf {
    if cfg!(windows) {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOMEDRIVE").and_then(|hd| std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp))))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::env::var("HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from("."))
    }
}

/// Normalize a file path for the current OS.
/// - On Windows: converts Unix-style `/X/path` to `X:/path` and normalizes separators
/// - Handles both backslash and forward-slash paths
/// - Resolves `~` to home directory
fn normalize_file_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');

    // Expand ~ to home directory
    let expanded = if trimmed.starts_with('~') {
        let home = user_home_dir();
        home.join(trimmed.trim_start_matches('~').trim_start_matches('/').trim_start_matches('\\'))
            .to_string_lossy()
            .to_string()
    } else {
        trimmed.to_string()
    };

    if cfg!(windows) {
        // Convert Unix-style /X/path to X:/path (e.g. /e/foo → E:/foo)
        let normalized = if expanded.len() >= 2
            && expanded.starts_with('/')
            && expanded.as_bytes().get(1).map_or(false, |b| b.is_ascii_alphabetic())
            && expanded.as_bytes().get(2).map_or(true, |b| *b == b'/' || *b == b'\\')
        {
            let drive = (expanded.as_bytes()[1] as char).to_ascii_uppercase();
            format!("{}:/{}", drive, &expanded[3..])
        } else {
            expanded
        };
        // Normalize separators to backslash for Windows
        normalized.replace('/', "\\")
    } else {
        // On Unix: normalize backslashes to forward slashes
        expanded.replace('\\', "/")
    }
}

fn get_project_name() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Try to find a project root marker nearby
    let markers = [
        "package.json", "Cargo.toml", "go.mod", "pyproject.toml", "setup.py",
        "Gemfile", "pom.xml", "build.gradle", "build.gradle.kts", "CMakeLists.txt",
        "Makefile",
        // .git last — avoid hijacking by a parent repo (e.g. dotfiles)
        ".git",
    ];
    let project_root = find_project_root(&cwd, &markers);

    project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn find_project_root(start: &std::path::Path, markers: &[&str]) -> std::path::PathBuf {
    let mut current = if start.is_dir() { start.to_path_buf() } else { start.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
    loop {
        for marker in markers {
            if current.join(marker).exists() {
                return current;
            }
        }
        if let Some(parent) = current.parent() {
            if parent == current { break; }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    // Fallback: use the start directory
    if start.is_dir() { start.to_path_buf() } else { start.parent().map(|p| p.to_path_buf()).unwrap_or_default() }
}

fn tool_reason(tool: &str, file: Option<&str>, cmd: Option<&str>) -> String {
    match tool {
        "run_command" | "run_tests" | "run_background" => {
            format!("Run: {}", cmd.unwrap_or("unknown command"))
        }
        "write_file" | "write_files" | "edit_file" => {
            format!("Edit: {}", file.unwrap_or("unknown file"))
        }
        "delete_file" => {
            format!("Delete: {}", file.unwrap_or("unknown file"))
        }
        "git_commit" => "Git commit".to_string(),
        "send_to_agent" => "Send message to agent".to_string(),
        _ => format!("Execute: {}", tool),
    }
}

fn is_parallel_safe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        // Read-only file inspection
        "read_file" | "read_files" | "list_dir" | "directory_tree" | "get_file_info"
        // Search & analysis
        | "search_in_dir" | "search_files" | "glob" | "analyze_project_structure"
        // Git read-only
        | "git_status" | "git_log" | "git_diff"
        // Web
        | "web_search" | "web_fetch"
        // Code graph (all read)
        | "code_graph_search" | "code_graph_callers" | "code_graph_callees"
        | "code_graph_explore" | "code_graph_file" | "code_graph_stats"
        // Knowledge read
        | "read_knowledge"
        // Skill inspection
        | "list_skills" | "match_skills"
        // Job inspection
        | "job_output" | "list_jobs"
        // Env
        | "get_env_info"
        // Background spawn (returns immediately)
        | "run_background" | "spawn_process"
    )
}

fn make_tool(name: &str, desc: &str, schema: serde_json::Value) -> serde_json::Value {
    json!({"name": name, "description": desc, "input_schema": schema})
}

fn schema_obj(props: serde_json::Value, required: &[&str]) -> serde_json::Value {
    let mut s = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        s["required"] = json!(required);
    }
    s
}

// ========== Code Graph Tool Implementations ==========

async fn tool_build_code_graph(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut cg = code_graph.lock().unwrap();

        // Try loading existing graph first
        match cg.load(&path) {
            Ok(true) => {
                let stats = cg.stats.clone();
                return json!({
                    "success": true,
                    "loaded": true,
                    "stats": stats,
                    "message": format!("已加载已有图谱：{} 节点，{} 边，{} 文件（上次构建耗时 {} ms）",
                        stats.total_nodes, stats.total_edges, stats.total_files, stats.build_time_ms)
                }).to_string();
            }
            Ok(false) => {}, // No saved graph, build new
            Err(e) => eprintln!("[code_graph] Load failed: {}", e),
        }

        // Build new graph
        match cg.build(&path) {
            Ok(stats) => json!({
                "success": true,
                "loaded": false,
                "stats": stats,
                "message": format!("图谱构建完成并已保存：{} 节点，{} 边，{} 文件，耗时 {} ms",
                    stats.total_nodes, stats.total_edges, stats.total_files, stats.build_time_ms)
            }).to_string(),
            Err(e) => json!({"success": false, "error": e}).to_string(),
        }
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_sync(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let path = normalize_file_path(params["path"].as_str().unwrap_or("."));
    let files: Vec<String> = params["files"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if files.is_empty() {
        return json!({"success": false, "error": "files list is required"}).to_string();
    }
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let mut cg = code_graph.lock().unwrap();
        match cg.sync(&path, &files) {
            Ok(stats) => json!({
                "success": true,
                "stats": stats,
                "synced_files": files.len(),
                "message": format!("增量同步完成：{} 文件，图谱现含 {} 节点 {} 边",
                    files.len(), stats.total_nodes, stats.total_edges)
            }).to_string(),
            Err(e) => json!({"success": false, "error": e}).to_string(),
        }
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_search(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let query = params["query"].as_str().unwrap_or("");
    let top_k = params["top_k"].as_i64().unwrap_or(20) as usize;
    if query.is_empty() {
        return json!({"success": false, "error": "query is required"}).to_string();
    }
    let query = query.to_string();
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        let results = cg.search(&query, top_k);
        json!({"success": true, "count": results.len(), "results": results}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_callers(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let node_id = params["node_id"].as_str().unwrap_or("");
    let max_depth = params["max_depth"].as_i64().unwrap_or(1) as usize;
    if node_id.is_empty() {
        return json!({"success": false, "error": "node_id is required"}).to_string();
    }
    let node_id = node_id.to_string();
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        let sub = cg.get_callers(&node_id, max_depth);
        json!({"success": true, "subgraph": sub}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_callees(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let node_id = params["node_id"].as_str().unwrap_or("");
    let max_depth = params["max_depth"].as_i64().unwrap_or(1) as usize;
    if node_id.is_empty() {
        return json!({"success": false, "error": "node_id is required"}).to_string();
    }
    let node_id = node_id.to_string();
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        let sub = cg.get_callees(&node_id, max_depth);
        json!({"success": true, "subgraph": sub}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_explore(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let query = params["query"].as_str().unwrap_or("");
    let max_nodes = params["max_nodes"].as_i64().unwrap_or(30) as usize;
    if query.is_empty() {
        return json!({"success": false, "error": "query is required"}).to_string();
    }
    let query = query.to_string();
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        let sub = cg.explore(&query, max_nodes);
        json!({"success": true, "subgraph": sub}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_file(params: &serde_json::Value, code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    let file_path = params["file_path"].as_str().unwrap_or("");
    if file_path.is_empty() {
        return json!({"success": false, "error": "file_path is required"}).to_string();
    }
    let file_path = file_path.to_string();
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        let symbols = cg.get_file_symbols(&file_path);
        json!({"success": true, "count": symbols.len(), "symbols": symbols}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_code_graph_stats(code_graph: Arc<StdMutex<CodeGraph>>) -> String {
    tokio::task::spawn_blocking(move || {
        let cg = code_graph.lock().unwrap();
        json!({"success": true, "stats": cg.stats}).to_string()
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_read_lints(
    params: &serde_json::Value,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let path: Option<String> = params["path"].as_str().map(|s| normalize_file_path(s));

    tokio::task::spawn_blocking(move || {
        let workspace = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT workspace FROM app_config", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .filter(|w: &String| !w.is_empty())
        };

        let result = if let Some(ref ws) = workspace {
            let mut mgr_guard = lsp_manager.lock().unwrap();
            let needs_init = mgr_guard.is_none();
            if needs_init {
                *mgr_guard = Some(LspManager::new(ws));
            }
            if let Some(ref mgr) = *mgr_guard {
                let diags = mgr.read_lints(path.as_deref());
                format_lints_result(&diags)
            } else {
                json!({"success": false, "error": "Failed to initialize LSP manager"}).to_string()
            }
        } else {
            json!({"success": false, "error": "No workspace set"}).to_string()
        };

        result
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_touch_file(
    params: &serde_json::Value,
    lsp_manager: Arc<StdMutex<Option<LspManager>>>,
    db: Arc<StdMutex<Connection>>,
) -> String {
    let file_path: String = normalize_file_path(params["file_path"].as_str().unwrap_or(""));

    tokio::task::spawn_blocking(move || {
        let workspace = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT workspace FROM app_config", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .filter(|w: &String| !w.is_empty())
        };

        if let Some(ref ws) = workspace {
            let mut mgr_guard = lsp_manager.lock().unwrap();
            if mgr_guard.is_none() {
                *mgr_guard = Some(LspManager::new(ws));
            }
            if let Some(ref mut mgr) = *mgr_guard {
                match mgr.touch_file(&file_path) {
                    Ok(diags) => {
                        if diags.is_empty() {
                            json!({"success": true, "diagnostics": []}).to_string()
                        } else {
                            json!({"success": true, "diagnostics": diags}).to_string()
                        }
                    }
                    Err(e) => json!({"success": false, "error": e}).to_string(),
                }
            } else {
                json!({"success": false, "error": "LSP manager init failed"}).to_string()
            }
        } else {
            json!({"success": false, "error": "No workspace set"}).to_string()
        }
    })
    .await
    .unwrap_or_else(|_| json!({"success": false, "error": "Task cancelled"}).to_string())
}

async fn tool_ask_choice(
    params: &serde_json::Value,
    session_id: i64,
    agent_type: &str,
    pending_asks: PendingAsks,
    app_handle: AppHandle,
) -> String {
    let questions: serde_json::Value = params.get("questions").cloned().unwrap_or(json!([]));
    let ask_id = format!("ask_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let (tx, rx) = tokio::sync::oneshot::channel();
    pending_asks.lock().unwrap().insert(ask_id.clone(), tx);

    let _ = app_handle.emit("ask_choice", json!({
        "id": ask_id,
        "session_id": session_id,
        "agent_type": agent_type,
        "questions": questions
    }));

    match rx.await {
        Ok(answers) => answers,
        Err(_) => json!({"cancelled": true}).to_string(),
    }
}

fn format_lints_result(diags: &[crate::lsp_types::FileDiagnostics]) -> String {
    if diags.is_empty() || diags.iter().all(|d| d.diagnostics.is_empty()) {
        return json!({"success": true, "message": "No linter errors found"}).to_string();
    }

    let mut output = String::from("Linter diagnostics:\n\n");
    for file_diag in diags {
        if file_diag.diagnostics.is_empty() { continue; }
        output.push_str(&format!("## {}\n", file_diag.file));
        output.push_str(&format_diagnostics(&file_diag.diagnostics));
        output.push_str("\n\n");
    }

    json!({"success": true, "diagnostics": output}).to_string()
}

fn code_graph_read_tools() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    vec![
        ("code_graph_search", "在代码图谱中按名称搜索符号（函数/类/接口等）",
            schema_obj(json!({"query": {"type": "string"}, "top_k": {"type": "integer"}}), &["query"])),
        ("code_graph_callers", "查找调用指定符号的所有位置（谁在调用它）",
            schema_obj(json!({"node_id": {"type": "string"}, "max_depth": {"type": "integer"}}), &["node_id"])),
        ("code_graph_callees", "查找指定符号调用了哪些符号",
            schema_obj(json!({"node_id": {"type": "string"}, "max_depth": {"type": "integer"}}), &["node_id"])),
        ("code_graph_explore", "深度探索：围绕查询词返回相关符号、调用关系、包含关系的完整子图。单次调用替代多次搜索",
            schema_obj(json!({"query": {"type": "string"}, "max_nodes": {"type": "integer"}}), &["query"])),
        ("code_graph_file", "查看某个文件中的所有符号",
            schema_obj(json!({"file_path": {"type": "string"}}), &["file_path"])),
        ("code_graph_stats", "获取已构建图谱的统计信息（文件数、节点数、语言分布等）",
            schema_obj(json!({}), &[])),
    ]
}

fn code_graph_explore_tools() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    vec![
        ("build_code_graph", "扫描并构建/加载项目代码图谱",
            schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("code_graph_sync", "增量更新图谱：接收变更文件列表，仅重新解析变更的文件。用于代码提交后快速同步",
            schema_obj(json!({"path": {"type": "string"}, "files": {"type": "array", "items": {"type": "string"}}}), &["path", "files"])),
    ]
}

fn get_agent_tools(agent_type: &str) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();

    // ===== TOOL GROUP DEFINITIONS =====

    // Read-only file inspection (all agents)
    let read_only_files: &[(&str, &str, serde_json::Value)] = &[
        ("read_file", "读取文件内容。offset: 起始行号(1-indexed)，limit: 最大行数。不传则读全文（>2MB 文件自动截断到前300行并提示用 offset/limit）", schema_obj(json!({"path": {"type": "string"}, "offset": {"type": "integer"}, "limit": {"type": "integer"}}), &["path"])),
        ("read_files", "批量读取多个文件", schema_obj(json!({"paths": {"type": "array", "items": {"type": "string"}}}), &["paths"])),
        ("list_dir", "列出目录内容", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("directory_tree", "递归列出目录树结构。maxDepth默认2，自动跳过node_modules/.git/target等目录", schema_obj(json!({"path": {"type": "string"}, "max_depth": {"type": "integer"}}), &["path"])),
        ("get_file_info", "获取文件信息（类型、大小、修改时间）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Search & analysis (plan / explore / work / review)
    let search_tools: &[(&str, &str, serde_json::Value)] = &[
        ("search_in_dir", "在目录中递归搜索文件内容，返回 path:line: text", schema_obj(json!({"path": {"type": "string"}, "pattern": {"type": "string"}}), &["path", "pattern"])),
        ("search_files", "按文件名搜索（大小写不敏感），匹配文件名而非内容", schema_obj(json!({"path": {"type": "string"}, "pattern": {"type": "string"}}), &["path", "pattern"])),
        ("glob", "按glob模式匹配文件，按修改时间倒序", schema_obj(json!({"pattern": {"type": "string"}, "path": {"type": "string"}, "limit": {"type": "integer"}}), &["pattern"])),
        ("analyze_project_structure", "分析项目顶层结构", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Git read-only (plan / explore / work / review)
    let git_read: &[(&str, &str, serde_json::Value)] = &[
        ("git_status", "获取Git仓库状态", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("git_log", "获取Git提交历史", schema_obj(json!({"path": {"type": "string"}, "count": {"type": "integer"}}), &["path"])),
        ("git_diff", "获取Git diff", schema_obj(json!({"path": {"type": "string"}, "target": {"type": "string"}}), &["path"])),
    ];

    // Write/edit tools (work only)
    let write_tools: &[(&str, &str, serde_json::Value)] = &[
        ("write_file", "创建或覆盖文件（含父目录）", schema_obj(json!({"path": {"type": "string"}, "content": {"type": "string"}}), &["path", "content"])),
        ("edit_file", "精确字符串替换。search必须唯一匹配，返回diff。大文件或多处修改用edit_lines", schema_obj(json!({"path": {"type": "string"}, "search": {"type": "string"}, "replace": {"type": "string"}}), &["path", "search", "replace"])),
        ("edit_lines", "按行号编辑。替换: start_line+end_line+content / 插入: start_line+content / 删除: start_line+end_line", schema_obj(json!({"path": {"type": "string"}, "start_line": {"type": "integer"}, "end_line": {"type": "integer"}, "content": {"type": "string"}}), &["path", "start_line"])),
        ("multi_edit", "原子性跨文件编辑。edits: [{path, search, replace}]。全部验证通过才写入，任一失败则回滚", schema_obj(json!({"edits": {"type": "array", "items": {"type": "object"}}}), &["edits"])),
        ("find_replace_in_files", "目录下批量查找替换（支持regex）", schema_obj(json!({"path": {"type": "string"}, "find": {"type": "string"}, "replace": {"type": "string"}, "use_regex": {"type": "boolean"}}), &["path", "find", "replace"])),
        ("create_directory", "创建目录（含父目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("move_file", "移动/重命名文件或目录", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
        ("delete_file", "删除单个文件（拒绝目录）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("copy_file", "复制文件或目录（递归）", schema_obj(json!({"source": {"type": "string"}, "destination": {"type": "string"}}), &["source", "destination"])),
    ];

    // Git write (work only)
    let _git_write: &[(&str, &str, serde_json::Value)] = &[
        ("git_commit", "创建Git提交", schema_obj(json!({"path": {"type": "string"}, "message": {"type": "string"}}), &["path", "message"])),
        ("git_branch", "列出Git分支", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("git_checkout", "切换Git分支", schema_obj(json!({"path": {"type": "string"}, "branch": {"type": "string"}}), &["path", "branch"])),
        ("git_stash", "暂存Git更改", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
        ("git_stash_pop", "恢复暂存的Git更改", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Command execution (work only)
    let command_tools: &[(&str, &str, serde_json::Value)] = &[
        ("run_command", "执行命令行指令", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}}), &["command"])),
        ("run_tests", "运行测试框架", schema_obj(json!({"path": {"type": "string"}, "test_framework": {"type": "string"}}), &["path", "test_framework"])),
        ("run_background", "后台运行长时间进程（dev server/build），返回PID和输出文件路径(out_file)。用job_output(out_file)读取实时输出", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "wait_sec": {"type": "integer"}}), &["command"])),
        ("kill_process", "按PID终止进程", schema_obj(json!({"pid": {"type": "number"}}), &["pid"])),
        ("job_output", "查询后台进程输出。out_file: run_background 返回的输出文件路径（推荐）；job_id: PID（旧方式）。tail_lines: 返回最后N行，默认200", schema_obj(json!({"job_id": {"type": "integer"}, "out_file": {"type": "string"}, "tail_lines": {"type": "integer"}}), &[])),
        ("list_jobs", "列出当前会话后台任务", schema_obj(json!({}), &[])),
    ];

    // Web & media (all agents)
    let web_tools: &[(&str, &str, serde_json::Value)] = &[
        ("web_search", "网络搜索，返回标题/URL/摘要", schema_obj(json!({"query": {"type": "string"}}), &["query"])),
        ("web_fetch", "获取URL内容并提取文本（去标签），上限32K字符", schema_obj(json!({"url": {"type": "string"}}), &["url"])),
        ("understand_image", "分析图片内容（JPEG/PNG/WebP）。传入prompt和image_url（本地文件路径或http URL）。同步返回分析结果，耗时5-30秒。不要对同一张图重复调用", schema_obj(json!({"prompt": {"type": "string"}, "image_url": {"type": "string"}}), &["prompt", "image_url"])),
    ];

    // Environment (all)
    let env_tools: &[(&str, &str, serde_json::Value)] = &[
        ("get_env_info", "获取开发环境信息（语言/框架/Git状态）", schema_obj(json!({"path": {"type": "string"}}), &["path"])),
    ];

    // Knowledge read (all)
    let knowledge_read: &[(&str, &str, serde_json::Value)] = &[
        ("read_knowledge", "读取项目知识库文件。file_name: 文件名（自动保存在工作目录对应的项目下）", schema_obj(json!({"file_name": {"type": "string"}}), &["file_name"])),
        ("list_knowledge", "列出项目知识库中的所有文件", schema_obj(json!({}), &[])),
    ];

    // Communication (all)
    let ask_tool = ("ask_choice", "向用户提问。用于需要用户选择或确认时。questions: [{id, question, options: [{id, text}], multi_select}]", schema_obj(json!({"questions": {"type": "array", "items": {"type": "object"}}}), &["questions"]));
    let comm_tools: &[(&str, &str, serde_json::Value)] = &[
        ("send_to_agent", "向其他智能体发送消息，发送即完成，无需等待回复。target_agent: front/plan/work/review/explore", schema_obj(json!({"target_agent": {"type": "string"}, "message": {"type": "string"}}), &["target_agent", "message"])),
    ];

    // Skill tools (front / plan / work / review - NOT explore)
    let skill_tools: &[(&str, &str, serde_json::Value)] = &[
        ("skill", "加载指定技能的完整操作指令", schema_obj(json!({"name": {"type": "string"}}), &["name"])),
        ("list_skills", "列出所有已加载的技能", schema_obj(json!({"source": {"type": "string"}}), &[])),
        ("match_skills", "根据描述关键词匹配技能", schema_obj(json!({"query": {"type": "string"}, "top_k": {"type": "integer"}}), &["query"])),
        ("execute_skill", "执行技能脚本", schema_obj(json!({"name": {"type": "string"}, "script": {"type": "string"}}), &["name"])),
    ];

    // Knowledge write (plan / explore / work)
    let kw = ("write_knowledge", "写入项目知识库文件。file_name: 文件名，content: 内容（自动保存在工作目录对应的项目下）", schema_obj(json!({"file_name": {"type": "string"}, "content": {"type": "string"}}), &["file_name", "content"]));

    // Lint tools (work / review)
    let lint = ("read_lints", "读取LSP诊断信息（类型错误、lint警告等）。可选传path参数过滤文件，不传则返回所有文件", schema_obj(json!({"path": {"type": "string"}}), &[]));

    fn add_tools(tools: &mut Vec<serde_json::Value>, defs: &[(&str, &str, serde_json::Value)]) {
        for (name, desc, schema) in defs {
            tools.push(make_tool(name, desc, schema.clone()));
        }
    }

    // ===== PER-ROLE ALLOCATION =====

    match agent_type {
        "front" => {
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, git_read);
            tools.push(make_tool("run_command", "执行命令行指令", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}}), &["command"])));
            tools.push(make_tool("run_background", "后台运行长时间进程，返回PID和输出文件路径。用job_output读取实时输出", schema_obj(json!({"command": {"type": "string"}, "path": {"type": "string"}, "wait_sec": {"type": "integer"}}), &["command"])));
            tools.push(make_tool("job_output", "查询后台进程输出。用run_background返回的out_file参数读取", schema_obj(json!({"out_file": {"type": "string"}, "tail_lines": {"type": "integer"}}), &[])));
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            tools.push(make_tool(ask_tool.0, ask_tool.1, ask_tool.2.clone()));
            add_tools(&mut tools, comm_tools);
            add_tools(&mut tools, skill_tools);
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        "plan" => {
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, git_read);
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            tools.push(make_tool(kw.0, kw.1, kw.2.clone()));
            tools.push(make_tool(ask_tool.0, ask_tool.1, ask_tool.2.clone()));
            add_tools(&mut tools, comm_tools);
            add_tools(&mut tools, skill_tools);
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        "explore" => {
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, git_read);
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            tools.push(make_tool(kw.0, kw.1, kw.2.clone()));
            add_tools(&mut tools, comm_tools);
            // Explore gets build + sync + read tools
            for (name, desc, schema) in code_graph_explore_tools() {
                tools.push(make_tool(name, desc, schema));
            }
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        "review" => {
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, git_read);
            // Review can commit after passing
            tools.push(make_tool("git_commit", "审查通过后提交代码", schema_obj(json!({"path": {"type": "string"}, "message": {"type": "string"}}), &["path", "message"])));
            tools.push(make_tool(lint.0, lint.1, lint.2.clone()));
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            add_tools(&mut tools, comm_tools);
            add_tools(&mut tools, skill_tools);
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        "work" => {
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, write_tools);
            add_tools(&mut tools, git_read);
            // Work keeps only git read-only (git_commit done by review)
            add_tools(&mut tools, command_tools);
            tools.push(make_tool(lint.0, lint.1, lint.2.clone()));
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            tools.push(make_tool(kw.0, kw.1, kw.2.clone()));
            add_tools(&mut tools, comm_tools);
            add_tools(&mut tools, skill_tools);
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        "ace" => {
            // Ace: all tools from all agents, minus send_to_agent
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, write_tools);
            add_tools(&mut tools, git_read);
            add_tools(&mut tools, _git_write);       // git_commit, branch, checkout, stash, stash_pop
            add_tools(&mut tools, command_tools);
            tools.push(make_tool(lint.0, lint.1, lint.2.clone()));
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            tools.push(make_tool(kw.0, kw.1, kw.2.clone()));
            tools.push(make_tool(ask_tool.0, ask_tool.1, ask_tool.2.clone()));  // ask_choice
            add_tools(&mut tools, skill_tools);
            for (name, desc, schema) in code_graph_explore_tools() {
                tools.push(make_tool(name, desc, schema));
            }
            for (name, desc, schema) in code_graph_read_tools() {
                tools.push(make_tool(name, desc, schema));
            }
        }
        _ => {
            // Fallback: same as front
            add_tools(&mut tools, read_only_files);
            add_tools(&mut tools, search_tools);
            add_tools(&mut tools, git_read);
            add_tools(&mut tools, web_tools);
            add_tools(&mut tools, env_tools);
            add_tools(&mut tools, knowledge_read);
            add_tools(&mut tools, comm_tools);
            add_tools(&mut tools, skill_tools);
        }
    }

    tools
}