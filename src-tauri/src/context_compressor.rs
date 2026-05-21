// Context Compressor — per-agent compression of api_messages when approaching token limit
//
// When api_messages reaches 80% of the 128K context window, the middle messages
// are compressed into a structured summary. The summary replaces the compressed
// messages directly in the api_messages array.

use serde_json::Value;

// MiniMax M2.7 context window: 204K tokens
const MAX_CONTEXT_TOKENS: usize = 204 * 1024;
const COMPRESS_THRESHOLD: f64 = 0.8;

// Token estimation using character-class weighting.
// CJK characters are token-dense (~1.5 chars/token). ASCII text is sparse (~4 chars/token).
// JSON structural characters are individually significant and counted at 1:1.
fn estimate_tokens(messages: &[Value]) -> usize {
    let mut cjk = 0usize;
    let mut ascii_alpha = 0usize;
    let mut json_structural = 0usize;
    let mut other = 0usize;

    for m in messages {
        for ch in serde_json::to_string(m).unwrap_or_default().chars() {
            match ch {
                '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
                | '\u{3400}'..='\u{4DBF}' // CJK Extension A
                | '\u{20000}'..='\u{2A6DF}' // CJK Extension B
                | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
                | '\u{3040}'..='\u{309F}' // Hiragana
                | '\u{30A0}'..='\u{30FF}' // Katakana
                | '\u{AC00}'..='\u{D7AF}' // Hangul
                => cjk += 1,
                '{' | '}' | '[' | ']' | '"' | ':' | ',' => json_structural += 1,
                'a'..='z' | 'A'..='Z' | '0'..='9' => ascii_alpha += 1,
                _ => other += 1,
            }
        }
    }

    // CJK: ~1.5 chars per token, ASCII text: ~4 chars per token,
    // JSON structural: 1 char per token, other: ~3 chars per token
    (cjk as f64 / 1.5) as usize
        + ascii_alpha / 4
        + json_structural
        + other / 3
}

/// Compress api_messages in-place when token budget exceeds 80%.
/// Always keeps the first message (oldest history) and the last N messages.
/// Middle messages are replaced with a single summary user message.
/// Note: system prompt is sent as top-level `system` field, not in messages.
pub fn compress_context(agent_type: &str, messages: &mut Vec<Value>) {
    let tokens = estimate_tokens(messages);
    let threshold = (MAX_CONTEXT_TOKENS as f64 * COMPRESS_THRESHOLD) as usize;
    if tokens < threshold {
        return;
    }

    let keep_recent = match agent_type {
        "front" => 8,
        "plan" => 4,
        "work" => 3,
        "review" => 3,
        "explore" => 3,
        _ => 4,
    };

    // Need at least keep_recent + 2 messages (system + middle) to be worth compressing
    if messages.len() <= keep_recent + 3 {
        return;
    }

    let split_idx = messages.len() - keep_recent;
    let first_msg = messages[0].clone();
    let recent: Vec<Value> = messages[split_idx..].to_vec();

    // Build summary from middle messages
    let summary = build_summary(agent_type, &messages[1..split_idx]);

    messages.clear();
    messages.push(first_msg);
    messages.push(serde_json::json!({
        "role": "user",
        "content": summary
    }));
    messages.extend(recent);

    let new_tokens = estimate_tokens(messages);
    eprintln!(
        "[compress] {}: {} → {} tokens (threshold: {})",
        agent_type, tokens, new_tokens, threshold
    );
}

fn build_summary(agent_type: &str, messages: &[Value]) -> String {
    let mut summary = String::from("## 上下文摘要\n\n");
    summary.push_str(&format!("已压缩 {} 条历史消息。\n\n", messages.len()));

    // Collect categorized information from messages
    let mut user_requests: Vec<String> = Vec::new();
    let mut agent_dispatches: Vec<String> = Vec::new();
    let mut files_touched: Vec<String> = Vec::new();
    let mut commands_run: Vec<String> = Vec::new();
    let mut git_actions: Vec<String> = Vec::new();
    let mut tool_uses: Vec<String> = Vec::new();
    let mut assistant_texts: Vec<String> = Vec::new();
    let mut graph_ops: Vec<String> = Vec::new();

    for msg in messages {
        let role = msg["role"].as_str().unwrap_or("");
        let content = &msg["content"];

        match role {
            "user" => {
                if let Some(text) = content.as_str() {
                    let t = text.trim();
                    if !t.is_empty() {
                        user_requests.push(truncate(t, 300));
                    }
                } else if let Some(blocks) = content.as_array() {
                    for block in blocks {
                        if block["type"] == "tool_result" {
                            let tool_id = block["tool_use_id"].as_str().unwrap_or("");
                            let result_text = block["content"].as_str().unwrap_or("");
                            let short_result = truncate(result_text, 200);
                            files_touched.push(format!("tool_result[{}]: {}", tool_id, short_result));
                        }
                    }
                }
            }
            "assistant" => {
                if let Some(blocks) = content.as_array() {
                    for block in blocks {
                        match block["type"].as_str().unwrap_or("") {
                            "text" => {
                                let text = block["text"].as_str().unwrap_or("");
                                if !text.is_empty() {
                                    assistant_texts.push(truncate(text, 500));
                                }
                            }
                            "tool_use" => {
                                let name = block["name"].as_str().unwrap_or("");
                                let input = &block["input"];
                                tool_uses.push(name.to_string());

                                match name {
                                    "send_to_agent" => {
                                        let target = input["target_agent"].as_str().unwrap_or("");
                                        let message = input["message"].as_str().unwrap_or("");
                                        agent_dispatches.push(format!(
                                            "→ {}: {}",
                                            target,
                                            truncate(message, 200)
                                        ));
                                    }
                                    "read_file" | "write_file" | "edit_file" | "delete_file"
                                    | "move_file" | "copy_file" | "create_directory" => {
                                        let path = input["file_path"].as_str()
                                            .or_else(|| input["path"].as_str())
                                            .unwrap_or("");
                                        files_touched.push(format!(
                                            "{}: {}",
                                            name,
                                            truncate(path, 150)
                                        ));
                                    }
                                    "run_command" | "run_tests" | "run_background" => {
                                        let cmd = input["command"].as_str().unwrap_or("");
                                        commands_run.push(format!("{}: {}", name, truncate(cmd, 150)));
                                    }
                                    "git_commit" | "git_diff" | "git_status" | "git_log" => {
                                        let desc = input["message"].as_str()
                                            .or_else(|| input["description"].as_str())
                                            .unwrap_or("");
                                        git_actions.push(format!("{}: {}", name, truncate(desc, 150)));
                                    }
                                    "build_code_graph" | "code_graph_sync"
                                    | "code_graph_search" | "code_graph_explore"
                                    | "code_graph_callers" | "code_graph_callees"
                                    | "code_graph_file" | "code_graph_stats" => {
                                        let query = input["query"].as_str()
                                            .or_else(|| input["file_path"].as_str())
                                            .or_else(|| input["changed_files"].as_str())
                                            .unwrap_or("");
                                        graph_ops.push(format!("{}: {}", name, truncate(query, 150)));
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Format summary based on agent type's role
    match agent_type {
        "front" => {
            // Top priority: what the user is asking for right now
            if !user_requests.is_empty() {
                summary.push_str("### 对话记录\n");
                for r in &user_requests {
                    summary.push_str(&format!("- 用户：{}\n", r));
                }
                summary.push('\n');
            }
            // Who was dispatched and why
            if !agent_dispatches.is_empty() {
                summary.push_str("### 调度记录\n");
                for d in &agent_dispatches {
                    summary.push_str(&format!("- {}\n", d));
                }
                summary.push('\n');
            }
            // What was discovered about the project
            if !graph_ops.is_empty() {
                summary.push_str("### 项目分析\n");
                for g in &graph_ops {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
            // Key outcomes and decisions
            if !assistant_texts.is_empty() {
                summary.push_str("### 关键结果\n");
                // Take the most recent significant replies (last 5)
                for t in assistant_texts.iter().rev().take(5) {
                    summary.push_str(&format!("- {}\n", t));
                }
                summary.push('\n');
            }
            if !files_touched.is_empty() {
                summary.push_str("### 涉及文件\n");
                // Deduplicate
                let mut seen = std::collections::HashSet::new();
                for f in &files_touched {
                    if seen.insert(f) {
                        summary.push_str(&format!("- {}\n", f));
                    }
                }
                summary.push('\n');
            }
            if !git_actions.is_empty() {
                summary.push_str("### Git\n");
                for g in &git_actions {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
        }
        "plan" => {
            if !user_requests.is_empty() {
                summary.push_str("### 需求\n");
                for r in &user_requests {
                    summary.push_str(&format!("- {}\n", r));
                }
                summary.push('\n');
            }
            if !graph_ops.is_empty() {
                summary.push_str("### 项目分析\n");
                for g in &graph_ops {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
            if !assistant_texts.is_empty() {
                summary.push_str("### 计划内容\n");
                for t in assistant_texts.iter().rev().take(3) {
                    summary.push_str(&format!("- {}\n", t));
                }
                summary.push('\n');
            }
            if !agent_dispatches.is_empty() {
                summary.push_str("### 已调度\n");
                for d in &agent_dispatches {
                    summary.push_str(&format!("- {}\n", d));
                }
                summary.push('\n');
            }
        }
        "work" => {
            if !user_requests.is_empty() {
                summary.push_str("### 任务\n");
                for r in &user_requests {
                    summary.push_str(&format!("- {}\n", r));
                }
                summary.push('\n');
            }
            if !files_touched.is_empty() {
                summary.push_str("### 文件操作\n");
                for f in &files_touched {
                    summary.push_str(&format!("- {}\n", f));
                }
                summary.push('\n');
            }
            if !commands_run.is_empty() {
                summary.push_str("### 命令执行\n");
                for c in &commands_run {
                    summary.push_str(&format!("- {}\n", c));
                }
                summary.push('\n');
            }
            if !git_actions.is_empty() {
                summary.push_str("### Git 操作\n");
                for g in &git_actions {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
            // Collect all tool use for work (they matter for knowing what was done)
            if !tool_uses.is_empty() {
                summary.push_str("### 已使用的工具\n");
                let mut deduped: Vec<&String> = tool_uses.iter().collect();
                deduped.sort();
                deduped.dedup();
                summary.push_str(&format!("- {}\n", deduped.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                summary.push('\n');
            }
        }
        "review" => {
            if !git_actions.is_empty() {
                summary.push_str("### Git 操作\n");
                for g in &git_actions {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
            if !files_touched.is_empty() {
                summary.push_str("### 涉及文件\n");
                for f in &files_touched {
                    summary.push_str(&format!("- {}\n", f));
                }
                summary.push('\n');
            }
            if !assistant_texts.is_empty() {
                summary.push_str("### 审查结果\n");
                for t in assistant_texts.iter().rev().take(2) {
                    summary.push_str(&format!("- {}\n", t));
                }
                summary.push('\n');
            }
        }
        "explore" => {
            if !graph_ops.is_empty() {
                summary.push_str("### 图谱操作\n");
                for g in &graph_ops {
                    summary.push_str(&format!("- {}\n", g));
                }
                summary.push('\n');
            }
            if !files_touched.is_empty() {
                summary.push_str("### 已分析文件\n");
                for f in &files_touched {
                    summary.push_str(&format!("- {}\n", f));
                }
                summary.push('\n');
            }
            if !assistant_texts.is_empty() {
                summary.push_str("### 探索发现\n");
                for t in assistant_texts.iter().rev().take(3) {
                    summary.push_str(&format!("- {}\n", t));
                }
                summary.push('\n');
            }
        }
        _ => {
            // Generic fallback
            if !user_requests.is_empty() {
                summary.push_str("### 用户消息\n");
                for r in &user_requests {
                    summary.push_str(&format!("- {}\n", r));
                }
                summary.push('\n');
            }
            if !assistant_texts.is_empty() {
                summary.push_str("### 回复要点\n");
                for t in assistant_texts.iter().rev().take(3) {
                    summary.push_str(&format!("- {}\n", t));
                }
                summary.push('\n');
            }
            if !tool_uses.is_empty() {
                summary.push_str("### 工具使用\n");
                let mut deduped: Vec<&String> = tool_uses.iter().collect();
                deduped.sort();
                deduped.dedup();
                summary.push_str(&format!("- {}\n", deduped.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                summary.push('\n');
            }
        }
    }

    summary
}

fn truncate(s: &str, max_len: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}…", truncated)
    }
}
