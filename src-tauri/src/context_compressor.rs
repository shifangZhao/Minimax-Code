// Context Compressor — per-agent compression of api_messages when approaching token limit.
//
// Middle messages are replaced with a model-generated structured summary.
// The model is asked to preserve task state, decisions, files touched, and pending work.

use serde_json::Value;

const SUMMARIZE_SYSTEM: &str = r#"你是上下文压缩器。你的任务是把对话历史压缩为结构化摘要，保留后续任务需要的全部关键信息。

## 必须保留
- 用户原始需求（完整保留，不缩写）
- 已完成的步骤和结果
- 当前进行到哪一步
- 修改了哪些文件、为什么改
- 运行过哪些命令、结果摘要
- 关键发现、决策、注意事项
- 尚未完成的任务

## 格式
输出纯文本，使用以下结构：
### 当前任务
[用户需求简述]

### 已完成
- [每步做了什么，结果如何]

### 涉及文件
- path: [修改原因]

### 待完成
- [还没做的事]

### 注意事项
- [关键发现、限制条件、踩过的坑]

不要加解释，直接输出摘要。"#;

/// Ask the model to summarize the given messages. Returns the summary string.
/// Uses the same Anthropic-compatible endpoint as the main chat.
pub async fn summarize_with_model(
    agent_type: &str,
    messages: &[Value],
    api_key: &str,
    api_url: &str,
    messages_path: &str,
    model: &str,
) -> String {
    let messages_json = serde_json::to_string(messages).unwrap_or_default();
    let label = if agent_type == "ace" { "全栈智能体" } else { "智能体" };
    let user_content = format!(
        "请压缩以下 {} {} 的历史对话：\n\n{}",
        agent_type, label, messages_json
    );

    let body = serde_json::json!({
        "model": model,
        "system": SUMMARIZE_SYSTEM,
        "messages": [{"role": "user", "content": user_content}],
        "max_tokens": 2048,
        "temperature": 0.3,
        "stream": false,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", api_url, messages_path))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            match r.json::<Value>().await {
                Ok(json) => {
                    // Anthropic format: content[0].text
                    let summary = json["content"][0]["text"]
                        .as_str()
                        .or_else(|| json["choices"][0]["message"]["content"].as_str())
                        .unwrap_or("")
                        .to_string();
                    if summary.is_empty() {
                        eprintln!("[summarize] Model returned empty summary");
                        "## 上下文摘要\n\n压缩失败，模型返回空内容。".to_string()
                    } else {
                        summary
                    }
                }
                Err(e) => {
                    eprintln!("[summarize] Failed to parse response: {}", e);
                    format!("## 上下文摘要\n\n压缩失败: {}", e)
                }
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            eprintln!("[summarize] API error {}: {}", status, body);
            format!("## 上下文摘要\n\n压缩失败: HTTP {} {}", status, body)
        }
        Err(e) => {
            eprintln!("[summarize] Request failed: {}", e);
            format!("## 上下文摘要\n\n压缩失败: {}", e)
        }
    }
}

// ── Token estimation ──

const SAMPLE_BOUND: usize = 4096;
const FULL_SCAN_BOUND: usize = 16384;

pub fn estimate_tokens(messages: &[Value]) -> usize {
    let mut total = 0usize;
    for m in messages {
        let text = serde_json::to_string(m).unwrap_or_default();
        total += estimate_one(&text);
    }
    total
}

pub fn estimate_request_tokens(
    messages: &[Value],
    system_json: &str,
    tools_json: &str,
) -> usize {
    let msg_tokens = estimate_tokens(messages);
    let system_tokens = estimate_one(system_json);
    let tools_tokens = estimate_one(tools_json);
    msg_tokens.saturating_add(system_tokens).saturating_add(tools_tokens)
}

fn estimate_one(text: &str) -> usize {
    let len = text.chars().count();
    if len <= FULL_SCAN_BOUND {
        return count_tokens_from_chars(text);
    }
    let head: String = text.chars().take(SAMPLE_BOUND).collect();
    let tail: String = text.chars().rev().take(SAMPLE_BOUND).collect::<Vec<_>>().into_iter().rev().collect();
    let sample_chars = head.chars().count() + tail.chars().count();
    let sample_tokens = count_tokens_from_chars(&head) + count_tokens_from_chars(&tail);
    if sample_chars == 0 { return 0; }
    let ratio = sample_tokens as f64 / sample_chars as f64;
    (len as f64 * ratio) as usize
}

fn count_tokens_from_chars(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut ascii_alpha = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        match ch {
            '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{20000}'..='\u{2A6DF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
            => cjk += 1,
            'a'..='z' | 'A'..='Z' | '0'..='9' => ascii_alpha += 1,
            _ => other += 1,
        }
    }
    (cjk as f64 / 2.0) as usize + ascii_alpha / 4 + other / 3
}

// ── Compression ──

/// Collapse Drain — aggressive compression when API returns context overflow.
/// `level`: 1 = moderately aggressive, 2 = keep only the very last exchange.
/// `summary`: model-generated summary.
pub fn compress_context_aggressive(agent_type: &str, messages: &mut Vec<Value>, level: usize, summary: String) -> bool {
    let keep_recent = match agent_type {
        "ace" => if level >= 2 { 2 } else { 4 },
        "front" => if level >= 2 { 2 } else { 3 },
        _ => if level >= 2 { 1 } else { 2 },
    };

    if messages.len() <= keep_recent + 3 {
        return false;
    }

    let old_len = messages.len();
    let split_idx = messages.len() - keep_recent;
    let first_msg = messages[0].clone();
    let recent: Vec<Value> = messages[split_idx..].to_vec();

    messages.clear();
    messages.push(first_msg);
    messages.push(serde_json::json!({ "role": "user", "content": summary }));
    messages.extend(recent);

    eprintln!("[collapse_drain] {} level={}: {} → {} messages", agent_type, level, old_len, messages.len());
    true
}

/// Compress api_messages in-place. The caller is responsible for deciding
/// whether compression is needed (the old heuristic threshold check is gone).
/// `summary`: model-generated summary.
pub fn compress_context(agent_type: &str, messages: &mut Vec<Value>, summary: String) {
    let keep_recent = match agent_type {
        "ace" => 10,
        "front" => 8,
        "plan" => 4,
        "work" => 3,
        "review" => 3,
        "explore" => 3,
        _ => 4,
    };

    if messages.len() <= keep_recent + 3 {
        return;
    }

    let old_len = messages.len();
    let split_idx = messages.len() - keep_recent;
    let first_msg = messages[0].clone();
    let recent: Vec<Value> = messages[split_idx..].to_vec();

    messages.clear();
    messages.push(first_msg);
    messages.push(serde_json::json!({ "role": "user", "content": summary }));
    messages.extend(recent);

    eprintln!("[compress] {}: {} → {} messages", agent_type, old_len, messages.len());
}
