//! llm.rs — client LLM streaming + function calling (chat-completions).
//!
//! Port bagian streaming voca/agent.py: kirim pesan + skema tools, terima delta
//! (teks & tool_calls) bertahap, cetak narasi, rakit tool_calls, kembalikan
//! (narasi, tool_calls). Ada retry untuk error koneksi sementara.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;

use crate::config::Limits;
use crate::provider::Provider;
use crate::ui;

// --- Pesan percakapan (format OpenAI) -------------------------------------
#[derive(Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Clone, Serialize, Deserialize)]
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
        Message { role: role.to_string(), content: Some(content.into()),
                  tool_calls: None, tool_call_id: None }
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
        Message { role: "tool".to_string(), content: Some(content),
                  tool_calls: None, tool_call_id: Some(id.to_string()) }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a Value>,
}

// --- Bentuk potongan stream SSE -------------------------------------------
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

/// Satu panggilan stream (dengan retry). Return (narasi, tool_calls).
pub async fn stream_once(
    client: &reqwest::Client,
    provider: &Provider,
    limits: &Limits,
    messages: &[Message],
    tools: &Value,
) -> Result<(String, Vec<ToolCall>)> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match try_stream(client, provider, limits, messages, tools).await {
            Ok(res) => return Ok(res),
            Err(e) => {
                if attempt >= limits.llm_max_retries {
                    return Err(e).context("LLM gagal setelah beberapa percobaan");
                }
                let delay = limits.llm_retry_base_delay * 2f64.powi((attempt - 1) as i32);
                ui::warn(&format!("koneksi LLM bermasalah ({e}); coba lagi dalam {delay:.0}s…"));
                tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
            }
        }
    }
}

struct AccTool {
    id: String,
    name: String,
    args: String,
}

async fn try_stream(
    client: &reqwest::Client,
    provider: &Provider,
    limits: &Limits,
    messages: &[Message],
    tools: &Value,
) -> Result<(String, Vec<ToolCall>)> {
    let api_key = provider.api_key.as_deref().context("provider tidak punya API key")?;
    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
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
        .await?
        .error_for_status()?;

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut full = String::new();
    let mut printed = false;
    let mut acc: BTreeMap<usize, AccTool> = BTreeMap::new();
    let mut stdout = std::io::stdout();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                continue;
            }
            let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) else { continue };
            let Some(choice) = parsed.choices.into_iter().next() else { continue };

            if let Some(piece) = choice.delta.content {
                if !piece.is_empty() {
                    if !printed {
                        ui::assistant_prefix();
                        printed = true;
                    }
                    print!("{piece}");
                    stdout.flush().ok();
                    full.push_str(&piece);
                }
            }
            for tc in choice.delta.tool_calls.unwrap_or_default() {
                let slot = acc.entry(tc.index).or_insert_with(|| AccTool {
                    id: String::new(),
                    name: String::new(),
                    args: String::new(),
                });
                if let Some(id) = tc.id {
                    slot.id = id;
                }
                if let Some(f) = tc.function {
                    if let Some(n) = f.name {
                        slot.name.push_str(&n);
                    }
                    if let Some(a) = f.arguments {
                        slot.args.push_str(&a);
                    }
                }
            }
        }
    }
    if printed {
        println!();
        println!();
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
