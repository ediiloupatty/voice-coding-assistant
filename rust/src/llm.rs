use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

use crate::app::AppEvent;
use crate::config::Limits;
use crate::provider::Provider;

// ── Message types (format OpenAI chat-completions) ───────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Message {
            role: role.to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: String, tool_calls: Vec<ToolCall>) -> Self {
        Message {
            role: "assistant".to_string(),
            content: Some(content),
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            tool_call_id: None,
        }
    }

    pub fn tool_result(id: &str, content: String) -> Self {
        Message {
            role: "tool".to_string(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(id.to_string()),
        }
    }
}

// ── Request / SSE response types ─────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a Value>,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}
#[derive(Deserialize)]
struct StreamChoice {
    delta: Delta,
}
#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}
#[derive(Deserialize)]
struct DeltaToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeltaFunction>,
}
#[derive(Deserialize)]
struct DeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

struct AccTool { id: String, name: String, args: String }

// ── Debug logging (aktif hanya saat VOCA_DEBUG=1) ────────────────────────────

fn debug_log(msg: &str) {
    if std::env::var("VOCA_DEBUG").as_deref() == Ok("1") {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true).open("voca.log")
        {
            let _ = std::io::Write::write_fmt(&mut f, format_args!("[llm] {msg}\n"));
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

pub async fn stream_to_channel(
    client: reqwest::Client,
    provider: Provider,
    limits: Limits,
    messages: Vec<Message>,
    tools: Value,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match try_stream(&client, &provider, &limits, &messages, &tools, &tx).await {
            Ok((full, tool_calls)) => {
                debug_log(&format!("selesai: {} karakter, {} tool", full.len(), tool_calls.len()));
                let _ = tx.send(AppEvent::LlmComplete(full, tool_calls));
                return;
            }
            Err(e) => {
                if attempt >= limits.llm_max_retries {
                    let _ = tx.send(AppEvent::LlmError(
                        format!("LLM failed after {attempt} attempts: {e}")
                    ));
                    return;
                }
                let delay = limits.llm_retry_base_delay
                    * 2f64.powi((attempt - 1) as i32);
                let _ = tx.send(AppEvent::LlmError(
                    format!("LLM connection issue ({e}); retrying in {delay:.0}s…")
                ));
                tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
            }
        }
    }
}

// ── Internal: satu percobaan streaming ──────────────────────────────────────

async fn try_stream(
    client: &reqwest::Client,
    provider: &Provider,
    limits: &Limits,
    messages: &[Message],
    tools: &Value,
    tx: &mpsc::UnboundedSender<AppEvent>,
) -> Result<(String, Vec<ToolCall>)> {
    let api_key = provider
        .api_key
        .as_deref()
        .context("provider has no API key")?;
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );

    debug_log(&format!("→ POST {url}"));

    let body = ChatRequest {
        model: &provider.model,
        messages,
        stream: true,
        temperature: limits.temperature,
        tools: Some(tools),
    };

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("failed to reach API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        debug_log(&format!("HTTP {status} ← {body}"));
        return Err(anyhow::anyhow!("HTTP {status}: {body}"));
    }

    // ── Parse SSE stream ─────────────────────────────────────────────────────
    let mut stream = resp.bytes_stream();
    let mut buf    = String::new();
    let mut full   = String::new();
    let mut acc: BTreeMap<usize, AccTool> = BTreeMap::new();

    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));

        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);

            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" { continue; }

            let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) else { continue };
            let Some(choice) = parsed.choices.into_iter().next() else { continue };

            if let Some(piece) = choice.delta.content {
                if !piece.is_empty() {
                    let _ = tx.send(AppEvent::LlmChunk(piece.clone()));
                    full.push_str(&piece);
                }
            }

            for tc in choice.delta.tool_calls.unwrap_or_default() {
                let slot = acc.entry(tc.index).or_insert_with(|| AccTool {
                    id: String::new(), name: String::new(), args: String::new(),
                });
                if let Some(id) = tc.id                   { slot.id.push_str(&id); }
                if let Some(f)  = tc.function {
                    if let Some(n) = f.name      { slot.name.push_str(&n); }
                    if let Some(a) = f.arguments { slot.args.push_str(&a); }
                }
            }
        }
    }

    let tool_calls: Vec<ToolCall> = acc
        .into_values()
        .map(|t| ToolCall {
            id: t.id,
            kind: "function".to_string(),
            function: FunctionCall {
                name: t.name,
                arguments: if t.args.is_empty() { "{}".to_string() } else { t.args },
            },
        })
        .collect();

    Ok((full, tool_calls))
}
