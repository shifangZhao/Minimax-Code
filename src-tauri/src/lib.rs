mod agent_service;
mod code_graph;
mod context_compressor;
mod lsp_client;
mod lsp_manager;
mod lsp_types;
mod mcp_service;
mod permission;
mod skill_service;
mod system_prompts;

pub(crate) const API_BASE_URL: &str = "https://api.minimaxi.com";
pub(crate) const SEARCH_TIMEOUT_SECS: u64 = 30;
pub(crate) const VLM_TIMEOUT_SECS: u64 = 60;
pub(crate) const MCP_HTTP_TIMEOUT_SECS: u64 = 30;

use agent_service::{AgentService, Message};
use code_graph::CodeGraph;
use lsp_manager::LspManager;
use mcp_service::{McpService, McpTool};
use permission::{PermissionService, PermissionMode, PermissionAction};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use skill_service::{Skill, SkillMatch, SkillService};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, RwLock};
use tauri::{State, Window, AppHandle};

struct AppState {
    db: Arc<Mutex<Connection>>,
    skill_service: Arc<SkillService>,
    mcp_service: Arc<RwLock<McpService>>,
    code_graph: Arc<Mutex<CodeGraph>>,
    lsp_manager: Arc<Mutex<Option<LspManager>>>,
    permission_service: Arc<Mutex<PermissionService>>,
    pending_asks: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupChat {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: i64,
    pub group_chat_id: i64,
    pub agent_type: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<String>,  // JSON string of tool_calls array
    #[serde(default)]
    pub thinking: Option<String>,  // thinking content
    #[serde(default)]
    pub attachments: Option<String>,  // JSON array of {name, path, kind}
    #[serde(default)]
    pub raw_json: Option<String>,  // full JSON of content block array for cache preservation
    pub created_at: String,
}

// ========== Window Commands ==========

#[tauri::command]
fn minimize_window(window: Window) {
    window.minimize().unwrap();
}

#[tauri::command]
fn maximize_window(window: Window) {
    if window.is_maximized().unwrap() {
        window.unmaximize().unwrap();
    } else {
        window.maximize().unwrap();
    }
}

#[tauri::command]
fn close_window(window: Window) {
    window.close().unwrap();
}

#[tauri::command]
fn is_maximized(window: Window) -> bool {
    window.is_maximized().unwrap_or(false)
}

// ========== Group Chat Commands ==========

#[tauri::command]
fn create_group_chat(state: State<AppState>, name: String) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("INSERT INTO group_chat (name) VALUES (?1)", [&name])
        .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn get_group_chats(state: State<AppState>) -> Result<Vec<GroupChat>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at FROM group_chat ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;
    let chats = stmt.query_map([], |row| {
        Ok(GroupChat {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
    Ok(chats)
}

#[tauri::command]
fn delete_group_chat(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // Delete all agent sessions and their messages first
    conn.execute("DELETE FROM chat_message WHERE session_id IN (SELECT id FROM agent_session WHERE group_chat_id = ?1)", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM agent_session WHERE group_chat_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM group_chat WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn rename_group_chat(state: State<AppState>, id: i64, name: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE group_chat SET name = ?1 WHERE id = ?2", rusqlite::params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ========== Agent Session Commands ==========

#[tauri::command]
fn create_agent_session(state: State<AppState>, group_chat_id: i64, agent_type: String) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO agent_session (group_chat_id, agent_type) VALUES (?1, ?2)",
        rusqlite::params![group_chat_id, agent_type],
    ).map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn get_agent_sessions(state: State<AppState>, group_chat_id: i64, agent_type: String) -> Result<Vec<AgentSession>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, group_chat_id, agent_type, created_at FROM agent_session WHERE group_chat_id = ?1 AND agent_type = ?2"
    ).map_err(|e| e.to_string())?;
    let sessions = stmt.query_map(rusqlite::params![group_chat_id, agent_type], |row| {
        Ok(AgentSession {
            id: row.get(0)?,
            group_chat_id: row.get(1)?,
            agent_type: row.get(2)?,
            created_at: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
    Ok(sessions)
}

// ========== Chat Message Commands ==========

#[tauri::command]
fn add_message(state: State<AppState>, session_id: i64, role: String, content: String, tool_calls: Option<String>, thinking: Option<String>, attachments: Option<String>, raw_json: Option<String>) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO chat_message (session_id, role, content, tool_calls, thinking, attachments, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![session_id, role, content, tool_calls, thinking, attachments, raw_json],
    ).map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn get_messages(state: State<AppState>, session_id: i64) -> Result<Vec<ChatMessage>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, tool_calls, thinking, attachments, raw_json, created_at FROM chat_message WHERE session_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| e.to_string())?;
    let messages = stmt.query_map([session_id], |row| {
        let created_at: String = row.get(8)?;
        Ok(ChatMessage {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            tool_calls: row.get(4)?,
            thinking: row.get(5)?,
            attachments: row.get(6)?,
            raw_json: row.get(7)?,
            created_at: created_at.replace(' ', "T") + "Z",
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
    Ok(messages)
}

#[tauri::command]
fn delete_message(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM chat_message WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn clear_session_history(state: State<AppState>, session_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM chat_message WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_minimax_api_key(state: State<AppState>, api_key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM minimax_api_key", [])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO minimax_api_key (api_key) VALUES (?1)",
        [&api_key],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_minimax_api_key(state: State<AppState>) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let result: Option<String> = conn.query_row(
        "SELECT api_key FROM minimax_api_key",
        [],
        |row| row.get(0)
    ).ok();
    Ok(result)
}

#[tauri::command]
async fn set_workspace(state: State<'_, AppState>, workspace: String) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO app_config (id, workspace) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET workspace = ?1",
            [&workspace],
        ).map_err(|e| e.to_string())?;
    } // conn MutexGuard dropped here, before .await

    // Load project skills for the new workspace
    state.skill_service.load_project_skills(&workspace).await;
    eprintln!("[set_workspace] Loaded project skills for: {}", workspace);
    Ok(())
}

#[tauri::command]
fn get_workspace(state: State<AppState>) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let result: Option<String> = conn.query_row(
        "SELECT workspace FROM app_config",
        [],
        |row| row.get(0)
    ).ok();
    Ok(result)
}

#[tauri::command]
fn set_model(state: State<AppState>, model: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO app_config (id, model) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET model = ?1",
        [&model],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_model(state: State<AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let result: String = conn.query_row(
        "SELECT model FROM app_config",
        [],
        |row| row.get(0)
    ).unwrap_or_else(|_| "MiniMax-M2.7".to_string());
    Ok(result)
}

// ========== File System Commands ==========

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file '{}': {}", path, e))
}

#[tauri::command]
fn write_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file '{}': {}", path, e))
}

#[tauri::command]
fn list_dir(path: String) -> Result<Vec<FileEntry>, String> {
    let entries = std::fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut result: Vec<FileEntry> = entries
        .filter_map(|e| e.ok())
        .map(|e| FileEntry {
            name: e.file_name().to_string_lossy().to_string(),
            path: e.path().to_string_lossy().to_string(),
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        })
        .collect();
    result.sort_by(|a, b| {
        if a.is_dir == b.is_dir {
            a.name.cmp(&b.name)
        } else if a.is_dir {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    Ok(result)
}

#[tauri::command]
fn create_dir(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create dir '{}': {}", path, e))
}

#[tauri::command]
fn remove_path(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if p.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("Failed to remove dir '{}': {}", path, e))
    } else {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to remove file '{}': {}", path, e))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[tauri::command]
fn run_command(command: String, cwd: Option<String>) -> Result<CommandOutput, String> {
    let (cmd, args) = if cfg!(windows) {
        ("cmd", vec!["/C".to_string(), command])
    } else {
        ("sh", vec!["-c".to_string(), command])
    };

    let mut process = Command::new(cmd);
    if let Some(dir) = cwd {
        process.current_dir(dir);
    }
    process.arg(&args[0]);
    for arg in &args[1..] {
        process.arg(arg);
    }

    let output = process.output().map_err(|e| format!("Failed to run command: {}", e))?;
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

// ========== Skill Commands ==========

#[tauri::command]
async fn list_skills(state: State<'_, AppState>, source: Option<String>) -> Result<Vec<Skill>, String> {
    Ok(state.skill_service.list_skills(source.as_deref()).await)
}

#[tauri::command]
async fn get_skill(state: State<'_, AppState>, name: String) -> Result<Option<Skill>, String> {
    Ok(state.skill_service.get_skill(&name).await)
}

#[tauri::command]
async fn get_skill_content(state: State<'_, AppState>, name: String) -> Result<Option<String>, String> {
    Ok(state.skill_service.get_skill_content(&name).await)
}

#[tauri::command]
async fn match_skills(state: State<'_, AppState>, query: String, top_k: Option<usize>) -> Result<Vec<SkillMatch>, String> {
    Ok(state.skill_service.match_skills(&query, top_k.unwrap_or(3)).await)
}

// ========== Web Search & Image Understanding ==========

#[tauri::command]
async fn web_search(query: String, state: State<'_, AppState>) -> Result<SearchResult, String> {
    let api_key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT api_key FROM minimax_api_key WHERE id = 1", [], |row| row.get::<_, String>(0))
            .map_err(|_| "No API key set".to_string())?
    };

    let client = reqwest::Client::new();
    let resp = client.post(format!("{}/v1/coding_plan/search", API_BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "q": query }))
        .timeout(std::time::Duration::from_secs(SEARCH_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let results = data.get("organic")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter().take(10).map(|item| {
                SearchResultItem {
                    title: item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    link: item.get("link").and_then(|l| l.as_str()).unwrap_or("").to_string(),
                    snippet: item.get("snippet").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                    date: item.get("date").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                }
            }).collect()
        })
        .unwrap_or_default();

    Ok(SearchResult { results })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub results: Vec<SearchResultItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub title: String,
    pub link: String,
    pub snippet: String,
    #[serde(default)]
    pub date: String,
}

#[tauri::command]
async fn understand_image(prompt: String, image_url: String, state: State<'_, AppState>) -> Result<String, String> {
    // Read and encode image to data URI
    let bytes = std::fs::read(&image_url).map_err(|e| format!("无法读取图片: {}", e))?;
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("图片超过 50MB 限制".to_string());
    }
    let ext = std::path::Path::new(&image_url)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let data_url = format!("data:{};base64,{}", mime, base64_encode_fast(&bytes));

    let api_key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT api_key FROM minimax_api_key WHERE id = 1", [], |row| row.get::<_, String>(0))
            .map_err(|_| "No API key set".to_string())?
    };

    let client = reqwest::Client::new();
    let resp = client.post(format!("{}/v1/coding_plan/vlm", API_BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "prompt": prompt,
            "image_url": data_url,
        }))
        .timeout(std::time::Duration::from_secs(VLM_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let description = data.get("content")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("VLM 返回异常: {}", data))?;
    Ok(description.to_string())
}

// ========== MCP Client Commands ==========

#[tauri::command]
async fn mcp_add_server(state: State<'_, AppState>, name: String, url: String) -> Result<(), String> {
    state.mcp_service.write().await.add_server(name, url).await
}

#[tauri::command]
async fn mcp_remove_server(state: State<'_, AppState>, name: String) -> Result<bool, String> {
    Ok(state.mcp_service.write().await.remove_server(&name).await)
}

#[tauri::command]
async fn mcp_list_servers(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    Ok(state.mcp_service.read().await.list_servers().await)
}

#[tauri::command]
async fn mcp_get_tools(state: State<'_, AppState>, server_name: String) -> Result<Vec<McpTool>, String> {
    Ok(state.mcp_service.read().await.get_server_tools(&server_name).await)
}

#[tauri::command]
async fn mcp_call_tool(state: State<'_, AppState>, server_name: String, tool_name: String, args: serde_json::Value) -> Result<serde_json::Value, String> {
    state.mcp_service.read().await.call_tool(&server_name, &tool_name, args).await
}

#[tauri::command]
async fn mcp_call_tool_any(state: State<'_, AppState>, tool_name: String, args: serde_json::Value) -> Result<serde_json::Value, String> {
    state.mcp_service.read().await.call_tool_any(&tool_name, args).await
}

// ========== Git Commands ==========

#[tauri::command]
fn git_status(repo_path: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, "status --porcelain")
}

#[tauri::command]
fn git_log(repo_path: String, count: Option<i32>) -> Result<CommandOutput, String> {
    let n = count.unwrap_or(20);
    run_git_command(&repo_path, &format!("log --oneline -{}", n))
}

#[tauri::command]
fn git_diff(repo_path: String, target: Option<String>) -> Result<CommandOutput, String> {
    let t = target.unwrap_or_else(|| "HEAD".to_string());
    run_git_command(&repo_path, &format!("diff {}", t))
}

#[tauri::command]
fn git_branch(repo_path: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, "branch -v")
}

#[tauri::command]
fn git_checkout(repo_path: String, branch: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, &format!("checkout {}", branch))
}

#[tauri::command]
fn git_commit(repo_path: String, message: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, &format!("commit -m \"{}\"", message))
}

#[tauri::command]
fn git_stash(repo_path: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, "stash push -m \"auto-stash\"")
}

#[tauri::command]
fn git_stash_pop(repo_path: String) -> Result<CommandOutput, String> {
    run_git_command(&repo_path, "stash pop")
}

fn run_git_command(repo_path: &str, args: &str) -> Result<CommandOutput, String> {
    let (cmd, shell_args) = if cfg!(windows) {
        ("cmd", vec!["/C".to_string(), format!("git -C \"{}\" {}", repo_path.replace("/", "\\"), args)])
    } else {
        ("sh", vec!["-c".to_string(), format!("git -C \"{}\" {}", repo_path, args)])
    };

    let output = Command::new(cmd)
        .args(&shell_args[1..])
        .output()
        .map_err(|e| format!("Failed to run git command: {}", e))?;
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

// ========== Code Search Commands ==========

#[tauri::command]
fn search_in_dir(path: String, pattern: String, file_filter: Option<String>) -> Result<Vec<SearchMatch>, String> {
    let filter = file_filter.unwrap_or_else(|| "*".to_string());
    let pattern_lower = pattern.to_lowercase();
    let mut matches = Vec::new();
    search_recursive(&path, &pattern_lower, &filter, &mut matches, 0, 10)?;
    Ok(matches)
}

fn search_recursive(path: &str, pattern: &str, filter: &str, matches: &mut Vec<SearchMatch>, depth: usize, max_depth: usize) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }

    let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|e| e.ok()) {
        let file_path = entry.path();
        let file_name = file_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden, node_modules, target, .git
        if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" || file_name == "__pycache__" {
            continue;
        }

        if file_path.is_dir() {
            search_recursive(&file_path.to_string_lossy(), pattern, filter, matches, depth + 1, max_depth)?;
        } else if file_path.is_file() {
            // Check filter match
            if !filter.is_empty() && filter != "*" {
                let ext = file_path.extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                let filter_exts: Vec<&str> = filter.split(',').map(|s| s.trim().trim_start_matches('.')).collect();
                if !filter_exts.iter().any(|f| f == &ext || ext.is_empty()) && !file_name.contains(&filter) {
                    continue;
                }
            }

            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains(pattern) {
                    let lines: Vec<&str> = content.lines().collect();
                    let mut line_matches = Vec::new();
                    for (i, line) in lines.iter().enumerate() {
                        if line.to_lowercase().contains(pattern) {
                            line_matches.push(LineMatch {
                                line_num: (i + 1) as i32,
                                content: line.trim().to_string(),
                            });
                            if line_matches.len() >= 5 {
                                break;
                            }
                        }
                    }
                    if !line_matches.is_empty() {
                        matches.push(SearchMatch {
                            file: file_path.to_string_lossy().to_string(),
                            lines: line_matches,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMatch {
    pub file: String,
    pub lines: Vec<LineMatch>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineMatch {
    pub line_num: i32,
    pub content: String,
}

// ========== Environment Detection ==========

#[tauri::command]
fn get_env_info(repo_path: String) -> Result<EnvInfo, String> {
    let mut info = EnvInfo {
        system: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        node_version: None,
        npm_version: None,
        python_version: None,
        rust_version: None,
        cargo_version: None,
        has_git: false,
        package_manager: None,
    };

    // Node/npm
    if let Ok(v) = run_simple_cmd("node --version") {
        info.node_version = Some(v.trim().to_string());
    }
    if let Ok(v) = run_simple_cmd("npm --version") {
        info.npm_version = Some(v.trim().to_string());
    }

    // Python
    if let Ok(v) = run_simple_cmd("python --version") {
        info.python_version = Some(v.trim().to_string());
    } else if let Ok(v) = run_simple_cmd("python3 --version") {
        info.python_version = Some(v.trim().to_string());
    }

    // Rust
    if let Ok(v) = run_simple_cmd("rustc --version") {
        info.rust_version = Some(v.trim().to_string());
    }
    if let Ok(v) = run_simple_cmd("cargo --version") {
        info.cargo_version = Some(v.trim().to_string());
    }

    // Git
    info.has_git = run_simple_cmd("git --version").is_ok();

    // Detect package manager
    let pkg_path = std::path::Path::new(&repo_path);
    if pkg_path.join("package.json").exists() {
        if pkg_path.join("pnpm-lock.yaml").exists() {
            info.package_manager = Some("pnpm".to_string());
        } else if pkg_path.join("yarn.lock").exists() {
            info.package_manager = Some("yarn".to_string());
        } else {
            info.package_manager = Some("npm".to_string());
        }
    } else if pkg_path.join("Cargo.toml").exists() {
        info.package_manager = Some("cargo".to_string());
    } else if pkg_path.join("requirements.txt").exists() || pkg_path.join("pyproject.toml").exists() {
        info.package_manager = Some("pip".to_string());
    }

    Ok(info)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvInfo {
    pub system: String,
    pub arch: String,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub python_version: Option<String>,
    pub rust_version: Option<String>,
    pub cargo_version: Option<String>,
    pub has_git: bool,
    pub package_manager: Option<String>,
}

fn run_simple_cmd(cmd: &str) -> Result<String, String> {
    let (shell, arg) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let output = Command::new(shell)
        .arg(arg)
        .arg(cmd)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err("Command failed".to_string())
    }
}

// ========== Project Structure Analysis ==========

#[tauri::command]
fn analyze_project_structure(repo_path: String) -> Result<ProjectStructure, String> {
    let mut structure = ProjectStructure {
        root_files: Vec::new(),
        src_dirs: Vec::new(),
        config_files: Vec::new(),
        has_tests: false,
        is_monorepo: false,
    };

    // Root level
    let root_entries = std::fs::read_dir(&repo_path).map_err(|e| e.to_string())?;
    for entry in root_entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        if is_dir {
            if name == "src" || name == "lib" || name == "packages" {
                structure.src_dirs.push(name.clone());
            }
            if name == "tests" || name == "__tests__" || name == "test" {
                structure.has_tests = true;
            }
            if name == "apps" || name == "packages" {
                structure.is_monorepo = true;
            }
        } else {
            if name.ends_with(".json") || name.ends_with(".toml") || name.ends_with(".yaml") || name.ends_with(".yml") || name == "Makefile" || name == ".gitignore" {
                structure.config_files.push(name.clone());
            } else if name == "package.json" || name == "Cargo.toml" || name == "go.mod" {
                structure.root_files.push(name.clone());
            }
        }
    }

    Ok(structure)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub root_files: Vec<String>,
    pub src_dirs: Vec<String>,
    pub config_files: Vec<String>,
    pub has_tests: bool,
    pub is_monorepo: bool,
}

// ========== Batch File Operations ==========

#[tauri::command]
fn read_files(paths: Vec<String>) -> Result<Vec<FileContent>, String> {
    let mut results = Vec::new();
    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(content) => results.push(FileContent {
                path,
                content,
                success: true,
                error: None,
            }),
            Err(e) => results.push(FileContent {
                path,
                content: String::new(),
                success: false,
                error: Some(e.to_string()),
            }),
        }
    }
    Ok(results)
}

#[tauri::command]
fn write_files(files: Vec<FileWrite>) -> Result<Vec<FileWriteResult>, String> {
    let mut results = Vec::new();
    for f in files {
        match write_file_safe(&f.path, &f.content) {
            Ok(()) => results.push(FileWriteResult {
                path: f.path,
                success: true,
                error: None,
            }),
            Err(e) => results.push(FileWriteResult {
                path: f.path,
                success: false,
                error: Some(e),
            }),
        }
    }
    Ok(results)
}

fn write_file_safe(path: &str, content: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileWrite {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileWriteResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

// ========== Process Management ==========

#[tauri::command]
fn spawn_process(command: String, cwd: Option<String>) -> Result<i32, String> {
    let (cmd, args) = if cfg!(windows) {
        ("cmd", vec!["/C".to_string(), "start".to_string(), "/B".to_string(), command])
    } else {
        ("sh", vec!["-c".to_string(), format!("{} &", command)])
    };

    let mut process = Command::new(cmd);
    if let Some(dir) = cwd {
        process.current_dir(dir);
    }
    process.arg(&args[0]);
    for arg in &args[1..] {
        process.arg(arg);
    }

    let child = process.spawn().map_err(|e| format!("Failed to spawn process: {}", e))?;
    Ok(child.id() as i32)
}

#[tauri::command]
fn kill_process(pid: i32) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .arg(&["-9".to_string(), pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ========== Multi-file Find & Replace ==========

#[tauri::command]
fn find_replace_in_files(dir: String, find: String, replace: String, file_filter: Option<String>, use_regex: bool) -> Result<Vec<FindReplaceResult>, String> {
    let filter = file_filter.unwrap_or_else(|| "*".to_string());
    let filter_exts: Vec<&str> = filter.split(',').map(|s| s.trim().trim_start_matches('.')).collect();
    let mut results = Vec::new();

    find_replace_recursive(&dir, &find, &replace, &filter_exts, use_regex, &mut results, 0, 10)?;

    Ok(results)
}

fn find_replace_recursive(dir: &str, find: &str, replace: &str, filter_exts: &[&str], use_regex: bool, results: &mut Vec<FindReplaceResult>, depth: usize, max_depth: usize) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        if path.is_dir() {
            find_replace_recursive(&path.to_string_lossy(), find, replace, filter_exts, use_regex, results, depth + 1, max_depth)?;
        } else if path.is_file() {
            let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if !filter_exts.is_empty() && !filter_exts.iter().any(|f| *f == ext || ext.is_empty()) && !name.contains(&filter_exts.iter().next().unwrap_or(&"*").to_string()) {
                if !name.contains(&filter_exts[0]) && filter_exts[0] != "*" {
                    continue;
                }
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (new_content, count) = if use_regex {
                match regex_replace(&content, find, replace) {
                    Ok((nc, c)) => (nc, c),
                    Err(_) => continue,
                }
            } else {
                let count = content.matches(find).count();
                (content.replace(find, replace), count)
            };
            if count > 0 {
                std::fs::write(&path, &new_content).map_err(|e| e.to_string())?;
                results.push(FindReplaceResult {
                    file: path.to_string_lossy().to_string(),
                    replacements: count,
                });
            }
        }
    }
    Ok(())
}

fn regex_replace(content: &str, pattern: &str, replace: &str) -> Result<(String, usize), String> {
    let re = regex::Regex::new(pattern).map_err(|e| e.to_string())?;
    let count = re.find_iter(content).count();
    let new_content = re.replace_all(content, replace).to_string();
    Ok((new_content, count))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FindReplaceResult {
    pub file: String,
    pub replacements: usize,
}

// ========== Patch / Diff Apply ==========

#[tauri::command]
fn apply_patch(repo_path: String, patch_content: String) -> Result<CommandOutput, String> {
    let patch_file = std::env::temp_dir().join("agent_patch.diff");
    std::fs::write(&patch_file, &patch_content).map_err(|e| e.to_string())?;

    let (cmd, args) = if cfg!(windows) {
        ("cmd", vec!["/C".to_string(), format!("git -C \"{}\" apply --3way \"{}\"", repo_path.replace("/", "\\"), patch_file.to_string_lossy().replace("/", "\\"))])
    } else {
        ("sh", vec!["-c".to_string(), format!("git -C \"{}\" apply --3way \"{}\"", repo_path, patch_file.to_string_lossy())])
    };

    let output = Command::new(cmd)
        .args(&args[1..])
        .output()
        .map_err(|e| format!("Failed to apply patch: {}", e))?;

    let _ = std::fs::remove_file(patch_file);
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

#[tauri::command]
fn create_patch(repo_path: String, target: Option<String>, output_path: Option<String>) -> Result<String, String> {
    let t = target.unwrap_or_else(|| "HEAD".to_string());
    let (_cmd, shell_arg) = if cfg!(windows) {
        ("cmd", format!("git -C \"{}\" {} diff {}", repo_path.replace("/", "\\"), t, if output_path.is_some() { format!("> \"{}\"", output_path.as_ref().unwrap().replace("/", "\\")) } else { String::new() }))
    } else {
        ("sh", format!("git -C \"{}\" diff {} {}", repo_path, t, if output_path.is_some() { format!("> \"{}\"", output_path.as_ref().unwrap()) } else { String::new() }))
    };

    let (shell, arg) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
    let output = Command::new(shell)
        .arg(arg)
        .arg(&shell_arg)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// ========== Test Runner ==========

#[tauri::command]
fn run_tests(repo_path: String, test_framework: String) -> Result<TestResult, String> {
    let (cmd, args) = match test_framework.as_str() {
        "jest" => ("cmd", vec!["/C".to_string(), "npm test -- --coverage=false --json".to_string()]),
        "pytest" => ("cmd", vec!["/C".to_string(), format!("python -m pytest --tb=short -q")]),
        "cargo" => ("cmd", vec!["/C".to_string(), "cargo test -- --nocapture".to_string()]),
        "npm" => ("cmd", vec!["/C".to_string(), "npm test -- --coverage=false".to_string()]),
        _ => return Err(format!("Unknown test framework: {}", test_framework)),
    };

    let mut process = Command::new(cmd);
    if test_framework == "pytest" || test_framework == "cargo" {
        process.current_dir(&repo_path);
    }
    process.arg(&args[0]);
    for arg in &args[1..] {
        process.arg(arg);
    }

    let output = process.output().map_err(|e| format!("Failed to run tests: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let (passed, failed, total, _duration) = parse_test_output(&stdout, &stderr, &test_framework);

    Ok(TestResult {
        passed,
        failed,
        total,
        duration_ms: 0,
        output: if stdout.is_empty() { stderr } else { stdout },
    })
}

fn parse_test_output(stdout: &str, stderr: &str, framework: &str) -> (i32, i32, i32, i64) {
    match framework {
        "jest" => {
            let passed = stdout.matches("\"passed\"").count() as i32;
            let failed = stdout.matches("\"failed\"").count() as i32;
            (passed, failed, passed + failed, 0)
        }
        "pytest" => {
            let failed = stderr.matches("FAILED").count() as i32;
            let passed = stdout.lines().filter(|l| l.contains(" PASSED")).count() as i32;
            (passed, failed, passed + failed, 0)
        }
        "cargo" | "npm" => {
            let failed = if stderr.contains("test result: FAILED") || stdout.contains("FAILED") { 1 } else { 0 };
            let passed = if stderr.contains("test result: ok") || stdout.contains("ok") { 1 } else { 0 };
            (passed, failed, passed + failed, 0)
        }
        _ => (0, 0, 0, 0),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestResult {
    pub passed: i32,
    pub failed: i32,
    pub total: i32,
    pub duration_ms: i64,
    pub output: String,
}

// ========== Code Modification (LLM-driven) ==========

#[tauri::command]
fn modify_files(files: Vec<FileModification>) -> Result<Vec<FileModificationResult>, String> {
    let mut results = Vec::new();
    for file in files {
        match std::fs::read_to_string(&file.path) {
            Ok(original) => {
                let new_content = if file.replacements.is_empty() {
                    file.new_content.unwrap_or(original)
                } else {
                    let mut content = original;
                    for r in &file.replacements {
                        content = content.replace(&r.find, &r.replace);
                    }
                    content
                };

                if let Some(parent) = std::path::Path::new(&file.path).parent() {
                    std::fs::create_dir_all(parent).ok();
                }

                match std::fs::write(&file.path, &new_content) {
                    Ok(()) => results.push(FileModificationResult {
                        path: file.path,
                        success: true,
                        error: None,
                    }),
                    Err(e) => results.push(FileModificationResult {
                        path: file.path,
                        success: false,
                        error: Some(e.to_string()),
                    }),
                }
            }
            Err(e) => results.push(FileModificationResult {
                path: file.path,
                success: false,
                error: Some(e.to_string()),
            }),
        }
    }
    Ok(results)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileModification {
    pub path: String,
    pub new_content: Option<String>,
    pub replacements: Vec<Replacement>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Replacement {
    pub find: String,
    pub replace: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileModificationResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

// ========== Agent Service (Rust Streaming) ==========

#[tauri::command]
async fn agent_chat_stream(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    agent_type: String,
    messages: String,
    system: Option<String>,
    workspace: Option<String>,
    session_id: i64,
) -> Result<(), String> {
    eprintln!("[agent_chat_stream] Called with session_id: {}", session_id);
    // Parse messages from JSON string
    let messages: Vec<Message> = serde_json::from_str(&messages)
        .map_err(|e| format!("Failed to parse messages: {}", e))?;

    // Get API key from database
    let api_key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let key: Option<String> = conn
            .query_row("SELECT api_key FROM minimax_api_key", [], |row| row.get(0))
            .ok();
        key.unwrap_or_default()
    };

    eprintln!("[agent_chat_stream] API key length: {}", api_key.len());
    if api_key.is_empty() {
        eprintln!("[agent_chat_stream] API key is empty!");
        return Err("API key not configured".to_string());
    }

    // Create agent service
    let service = AgentService::new(api_key, state.skill_service.clone(), state.mcp_service.clone(), state.db.clone(), state.code_graph.clone(), state.lsp_manager.clone(), state.permission_service.clone(), state.pending_asks.clone());
    eprintln!("[agent_chat_stream] AgentService created, spawning stream_chat");

    // Start streaming - spawn and await
    let _ = tokio::spawn(async move {
        service.stream_chat(&agent_type, messages, system, workspace, app_handle, session_id).await;
    });

    eprintln!("[agent_chat_stream] stream_chat spawned");

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[tauri::command]
async fn get_permission_mode(state: State<'_, AppState>) -> Result<String, String> {
    // Read from DB directly (source of truth)
    let mode: String = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT permission_mode FROM app_config", [], |row| row.get(0))
            .unwrap_or_else(|_| "normal".to_string())
    };
    // Sync in-memory service
    let p_mode = match mode.as_str() {
        "full" => PermissionMode::Full,
        "guarded" => PermissionMode::Guarded,
        _ => PermissionMode::Normal,
    };
    state.permission_service.lock().map_err(|e| e.to_string())?.set_mode(p_mode);
    Ok(serde_json::to_string(&p_mode).unwrap_or_else(|_| "\"normal\"".to_string()))
}

#[tauri::command]
async fn set_permission_mode(state: State<'_, AppState>, mode: String) -> Result<(), String> {
    let m: PermissionMode = serde_json::from_str(&format!("\"{}\"", mode))
        .map_err(|e| format!("Invalid mode: {}", e))?;
    let mode_raw = match m {
        PermissionMode::Full => "full",
        PermissionMode::Normal => "normal",
        PermissionMode::Guarded => "guarded",
    };
    state.permission_service.lock().map_err(|e| e.to_string())?.set_mode(m);
    // Persist to DB
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO app_config (id, permission_mode) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET permission_mode = ?1",
        [mode_raw],
    ).map_err(|e| e.to_string())?;
    eprintln!("[perm] Mode set to {} and persisted", mode_raw);
    Ok(())
}

#[tauri::command]
async fn respond_ask(
    state: State<'_, AppState>,
    id: String,
    answers: String,
) -> Result<(), String> {
    if let Some(tx) = state.pending_asks.lock().map_err(|e| e.to_string())?.remove(&id) {
        let _ = tx.send(answers);
    }
    Ok(())
}

#[tauri::command]
async fn respond_permission(
    state: State<'_, AppState>,
    id: String,
    tool: String,
    action: String,
    always: bool,
) -> Result<(), String> {
    let act: PermissionAction = match action.as_str() {
        "allow" => PermissionAction::Allow,
        "deny" => PermissionAction::Deny,
        _ => return Err(format!("Invalid action: {}", action)),
    };
    state.permission_service.lock().map_err(|e| e.to_string())?.resolve_pending(&id, &tool, act, always);
    Ok(())
}

#[tauri::command]
fn save_temp_file(name: String, data_url: String) -> Result<String, String> {
    let tmp = std::env::temp_dir().join("minimax-code");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    // Parse data URL: data:image/png;base64,xxxx
    let encoded = data_url
        .split(',')
        .nth(1)
        .ok_or("Invalid data URL")?;
    let bytes = base64_decode(encoded)?;
    let path = tmp.join(&name);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn base64_decode(encoded: &str) -> Result<Vec<u8>, String> {
    let clean: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let mut result = Vec::new();
    let bytes: Vec<u8> = clean.bytes().collect();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = char_to_val(Some(bytes[i]))?;
        let b1 = char_to_val(bytes.get(i + 1).copied())?;
        result.push((b0 << 2) | (b1 >> 4));
        if i + 2 < bytes.len() && bytes[i + 2] != b'=' {
            let b2 = char_to_val(Some(bytes[i + 2]))?;
            result.push((b1 << 4) | (b2 >> 2));
            if i + 3 < bytes.len() && bytes[i + 3] != b'=' {
                let b3 = char_to_val(Some(bytes[i + 3]))?;
                result.push((b2 << 6) | b3);
            }
        }
        i += 4;
    }
    Ok(result)
}

fn char_to_val(c: Option<u8>) -> Result<u8, String> {
    match c.unwrap_or(b'A') {
        b'A'..=b'Z' => Ok(c.unwrap() - b'A'),
        b'a'..=b'z' => Ok(c.unwrap() - b'a' + 26),
        b'0'..=b'9' => Ok(c.unwrap() - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("Invalid base64 char: {}", c.unwrap() as char)),
    }
}

fn base64_encode_fast(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let val = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((val >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((val >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 { result.push(CHARS[((val >> 6) & 0x3F) as usize] as char); } else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(val & 0x3F) as usize] as char); } else { result.push('='); }
    }
    result
}

#[tauri::command]
fn read_file_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("无法读取文件: {}", e))?;
    let ext = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/png",
    };
    Ok(format!("data:{};base64,{}", mime, base64_encode_fast(&bytes)))
}

fn init_user_dir() {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOMEDRIVE").and_then(|hd| std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp))))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::env::var("HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from("."))
    };
    let base = home.join(".minimaxcode");
    let _ = std::fs::create_dir_all(&base);
    let _ = std::fs::create_dir_all(base.join("project mem"));
    let _ = std::fs::create_dir_all(base.join("skills"));

    // Clean up old temp files (> 1 hour)
    let tmp = std::env::temp_dir().join("minimax-code");
    if tmp.exists() {
        if let Ok(entries) = std::fs::read_dir(&tmp) {
            let now = std::time::SystemTime::now();
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(age) = now.duration_since(meta.created().unwrap_or(now)) {
                        if age.as_secs() > 3600 {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }

    let mcp_config = base.join("mcp.json");
    if !mcp_config.exists() {
        let template = serde_json::json!({
            "mcpServers": {
                "_comment": "=== MCP Server 配置模板 ===",
                "_local_example": {
                    "type": "local",
                    "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem"],
                    "env": {},
                    "enabled": false,
                    "timeout": 30000
                },
                "_remote_example": {
                    "type": "remote",
                    "url": "https://example.com/mcp",
                    "headers": {},
                    "enabled": false,
                    "timeout": 30000
                }
            }
        });
        if let Ok(json) = serde_json::to_string_pretty(&template) {
            let _ = std::fs::write(&mcp_config, json);
            eprintln!("[init] Created {}", mcp_config.display());
        }
    }
    eprintln!("[init] User dir ready: {}", base.display());
}

pub fn run() {
    let app_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("minimax-code");
    std::fs::create_dir_all(&app_dir).ok();

    init_user_dir();
    let db_path = app_dir.join("minimax.db");

    let conn = Connection::open(&db_path).expect("Failed to open database");

    // Drop old chat_history table if exists (migration from old schema)
    conn.execute("DROP TABLE IF EXISTS chat_history", [])
        .expect("Failed to drop old chat_history table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_chat (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).expect("Failed to create group_chat table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_session (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_chat_id INTEGER NOT NULL,
            agent_type TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (group_chat_id) REFERENCES group_chat(id)
        )",
        [],
    ).expect("Failed to create agent_session table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_message (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_calls TEXT DEFAULT NULL,
            thinking TEXT DEFAULT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES agent_session(id)
        )",
        [],
    ).expect("Failed to create chat_message table");

    // Migration: add tool_calls column if it doesn't exist (for existing databases)
    let has_tool_calls: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('chat_message') WHERE name='tool_calls'",
        [],
        |row| row.get::<_, i32>(0)
    ).unwrap_or(0) > 0;
    if !has_tool_calls {
        conn.execute("ALTER TABLE chat_message ADD COLUMN tool_calls TEXT DEFAULT NULL", [])
            .expect("Failed to add tool_calls column");
    }

    // Migration: add thinking column if it doesn't exist
    let has_thinking: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('chat_message') WHERE name='thinking'",
        [],
        |row| row.get::<_, i32>(0)
    ).unwrap_or(0) > 0;
    if !has_thinking {
        conn.execute("ALTER TABLE chat_message ADD COLUMN thinking TEXT DEFAULT NULL", [])
            .expect("Failed to add thinking column");
    }

    // Migration: add attachments column for image/file metadata
    let has_attachments: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('chat_message') WHERE name='attachments'",
        [],
        |row| row.get::<_, i32>(0)
    ).unwrap_or(0) > 0;
    if !has_attachments {
        conn.execute("ALTER TABLE chat_message ADD COLUMN attachments TEXT DEFAULT NULL", [])
            .expect("Failed to add attachments column");
    }

    // Migration: add raw_json column for cache-aware interleaved message reconstruction
    let has_raw_json: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('chat_message') WHERE name='raw_json'",
        [],
        |row| row.get::<_, i32>(0)
    ).unwrap_or(0) > 0;
    if !has_raw_json {
        conn.execute("ALTER TABLE chat_message ADD COLUMN raw_json TEXT DEFAULT NULL", [])
            .expect("Failed to add raw_json column");
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS minimax_api_key (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            api_key TEXT NOT NULL
        )",
        [],
    ).expect("Failed to create minimax_api_key table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            workspace TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT 'MiniMax-M2.7',
            permission_mode TEXT NOT NULL DEFAULT 'normal'
        )",
        [],
    ).expect("Failed to create app_config table");

    // Migration: add column if upgrading from older schema
    let _ = conn.execute("ALTER TABLE app_config ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'normal'", []);

    // Load permission mode from DB
    let perm_mode: String = conn
        .query_row("SELECT permission_mode FROM app_config", [], |row| row.get(0))
        .unwrap_or_else(|_| "normal".to_string());
    let mut perm_svc = PermissionService::new();
    let p_mode = match perm_mode.as_str() {
        "full" => PermissionMode::Full,
        "guarded" => PermissionMode::Guarded,
        _ => PermissionMode::Normal,
    };
    perm_svc.set_mode(p_mode);

    let skill_service = Arc::new(SkillService::new());

    tauri::Builder::default()
        .manage(AppState {
            db: Arc::new(Mutex::new(conn)),
            skill_service: skill_service.clone(),
            mcp_service: Arc::new(RwLock::new(McpService::new())),
            code_graph: Arc::new(Mutex::new(CodeGraph::new())),
            lsp_manager: Arc::new(Mutex::new(None)),
            permission_service: Arc::new(Mutex::new(perm_svc)),
            pending_asks: Arc::new(Mutex::new(HashMap::new())),
        })
        .setup(move |_app| {
            // Set builtin skills root - use directory relative to executable
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();
            let builtin_skills = exe_dir.join("skills");
            skill_service.set_builtin_root(builtin_skills);

            // Load skills asynchronously in background
            let skill_service_clone = skill_service.clone();
            tauri::async_runtime::spawn(async move {
                skill_service_clone.load_all_skills().await;
                eprintln!("[startup] Skills loaded: {} skills available",
                    skill_service_clone.list_skills(None).await.len());
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            minimize_window,
            maximize_window,
            close_window,
            is_maximized,
            create_group_chat,
            get_group_chats,
            delete_group_chat,
            rename_group_chat,
            create_agent_session,
            get_agent_sessions,
            add_message,
            get_messages,
            delete_message,
            clear_session_history,
            set_minimax_api_key,
            get_minimax_api_key,
            set_workspace,
            get_workspace,
            save_temp_file,
            read_file_base64,
            set_permission_mode,
            get_permission_mode,
            respond_permission,
            respond_ask,
            set_model,
            get_model,
            agent_chat_stream,
            read_file,
            write_file,
            list_dir,
            create_dir,
            remove_path,
            run_command,
            git_status,
            git_log,
            git_diff,
            git_branch,
            git_checkout,
            git_commit,
            git_stash,
            git_stash_pop,
            search_in_dir,
            get_env_info,
            analyze_project_structure,
            read_files,
            write_files,
            spawn_process,
            kill_process,
            find_replace_in_files,
            apply_patch,
            create_patch,
            run_tests,
            modify_files,
            list_skills,
            get_skill,
            get_skill_content,
            match_skills,
            web_search,
            understand_image,
            mcp_add_server,
            mcp_remove_server,
            mcp_list_servers,
            mcp_get_tools,
            mcp_call_tool,
            mcp_call_tool_any
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}