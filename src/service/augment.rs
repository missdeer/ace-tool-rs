//! Augment API service - New and Old endpoints

use std::time::Instant;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::http_logger::{self, HttpRequestLog, HttpResponseLog};

use super::common::{
    is_chinese_text, parse_chat_history, render_enhance_prompt, replace_tool_names, ChatMessage,
};

/// Default model for prompt enhancement API
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5";

/// Node ID for NEW endpoint (matches augment.mjs promptEnhancer)
pub const NODE_ID_NEW: i32 = 0;

/// Node ID for OLD endpoint
pub const NODE_ID_OLD: i32 = 1;

/// User-Agent header value (matches augment.mjs format: augment.cli/{version}/{mode})
const USER_AGENT: &str = "augment.cli/0.12.0/mcp";

/// Redacted token placeholder for logging
const REDACTED_TOKEN: &str = "<redacted>";

/// Generate a unique request ID
fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a session ID (persistent for the lifetime of the process)
fn get_session_id() -> &'static str {
    use std::sync::OnceLock;
    static SESSION_ID: OnceLock<String> = OnceLock::new();
    SESSION_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Request payload for NEW prompt-enhancer endpoint (simplified, matches augment.mjs)
#[derive(Debug, Serialize)]
struct PromptEnhancerRequestNew {
    nodes: Vec<PromptNode>,
    chat_history: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<String>,
    model: String,
    mode: String,
}

/// Request payload for OLD chat-stream endpoint (full request with blobs)
#[derive(Debug, Serialize, Default)]
struct PromptEnhancerRequestOld {
    model: String,
    path: Option<String>,
    prefix: Option<String>,
    selected_code: Option<String>,
    suffix: Option<String>,
    message: Option<String>,
    chat_history: Vec<ChatMessage>,
    lang: Option<String>,
    blobs: BlobsPayload,
    user_guided_blobs: Vec<String>,
    context_code_exchange_request_id: Option<String>,
    external_source_ids: Vec<String>,
    disable_auto_external_sources: Option<bool>,
    user_guidelines: String,
    workspace_guidelines: String,
    feature_detection_flags: FeatureDetectionFlags,
    third_party_override: Option<serde_json::Value>,
    tool_definitions: Vec<serde_json::Value>,
    nodes: Vec<PromptNode>,
    mode: String,
    agent_memories: Option<String>,
    persona_type: Option<String>,
    rules: Vec<String>,
    silent: Option<bool>,
    enable_parallel_tool_use: Option<bool>,
    conversation_id: Option<String>,
    system_prompt: Option<String>,
}

#[derive(Debug, Serialize, Default)]
struct FeatureDetectionFlags {
    support_parallel_tool_use: Option<bool>,
}

#[derive(Debug, Serialize, Clone, Default)]
struct BlobsPayload {
    checkpoint_id: Option<String>,
    added_blobs: Vec<String>,
    deleted_blobs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PromptNode {
    id: i32,
    #[serde(rename = "type")]
    node_type: i32,
    text_node: TextNode,
}

#[derive(Debug, Serialize)]
struct TextNode {
    content: String,
}

/// Response from prompt-enhancer API
#[derive(Debug, Deserialize)]
struct PromptEnhancerResponse {
    text: Option<String>,
}

/// Call NEW /prompt-enhancer endpoint (simplified, matches augment.mjs)
pub async fn call_new_endpoint(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let chat_history = parse_chat_history(conversation_history);

    let payload = PromptEnhancerRequestNew {
        nodes: vec![PromptNode {
            id: NODE_ID_NEW,
            node_type: 0,
            text_node: TextNode {
                content: original_prompt.to_string(),
            },
        }],
        chat_history,
        conversation_id: None,
        model: DEFAULT_MODEL.to_string(),
        mode: "CHAT".to_string(),
    };

    let url = format!("{}/prompt-enhancer", config.base_url);
    let request_id = generate_request_id();
    let start_time = Instant::now();

    let http_request_log = if http_logger::is_enabled() {
        let request_body = serde_json::to_string(&payload).ok();
        Some(HttpRequestLog {
            method: "POST".to_string(),
            url: url.clone(),
            headers: http_logger::extract_headers_from_builder(
                "application/json",
                USER_AGENT,
                &request_id,
                get_session_id(),
                REDACTED_TOKEN,
            ),
            body: request_body,
        })
    } else {
        None
    };

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .header("x-request-id", &request_id)
        .header("x-request-session-id", get_session_id())
        .header("Authorization", format!("Bearer {}", config.token))
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let response_headers = if http_logger::is_enabled() {
                http_logger::extract_response_headers(&resp)
            } else {
                Vec::new()
            };
            let body_text = resp.text().await.unwrap_or_default();
            if let Some(ref req_log) = http_request_log {
                let response_log = HttpResponseLog {
                    status: status.as_u16(),
                    headers: response_headers,
                    body: Some(body_text.clone()),
                };
                http_logger::log_request(None, req_log, Some(&response_log), duration_ms, None);
            }
            handle_response_text(status.as_u16(), &body_text, false)
        }
        Err(e) => {
            let error_msg = e.to_string();
            if let Some(ref req_log) = http_request_log {
                http_logger::log_request(None, req_log, None, duration_ms, Some(&error_msg));
            }
            Err(anyhow!("Request failed: {}", error_msg))
        }
    }
}

/// Call OLD /chat-stream endpoint (full request with blobs)
pub async fn call_old_endpoint(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
    blob_names: &[String],
) -> Result<String> {
    let chat_history = parse_chat_history(conversation_history);

    let final_prompt = render_enhance_prompt(original_prompt)?;

    let is_chinese = is_chinese_text(original_prompt);
    let language_guideline = if is_chinese {
        "Please respond in Chinese (Simplified Chinese). 请用中文回复。".to_string()
    } else {
        String::new()
    };

    let mut sorted_blob_names = blob_names.to_vec();
    sorted_blob_names.sort();

    let payload = PromptEnhancerRequestOld {
        model: DEFAULT_MODEL.to_string(),
        path: None,
        prefix: None,
        selected_code: None,
        suffix: None,
        message: Some(final_prompt.clone()),
        chat_history,
        lang: None,
        blobs: BlobsPayload {
            checkpoint_id: None,
            added_blobs: sorted_blob_names,
            deleted_blobs: Vec::new(),
        },
        user_guided_blobs: Vec::new(),
        context_code_exchange_request_id: None,
        external_source_ids: Vec::new(),
        disable_auto_external_sources: None,
        user_guidelines: language_guideline,
        workspace_guidelines: String::new(),
        feature_detection_flags: FeatureDetectionFlags::default(),
        third_party_override: None,
        tool_definitions: Vec::new(),
        nodes: vec![PromptNode {
            id: NODE_ID_OLD,
            node_type: 0,
            text_node: TextNode {
                content: final_prompt,
            },
        }],
        mode: "CHAT".to_string(),
        agent_memories: None,
        persona_type: None,
        rules: Vec::new(),
        silent: None,
        enable_parallel_tool_use: None,
        conversation_id: None,
        system_prompt: None,
    };

    let url = format!("{}/chat-stream", config.base_url);
    let request_id = generate_request_id();
    let start_time = Instant::now();

    let http_request_log = if http_logger::is_enabled() {
        let request_body = serde_json::to_string(&payload).ok();
        Some(HttpRequestLog {
            method: "POST".to_string(),
            url: url.clone(),
            headers: http_logger::extract_headers_from_builder(
                "application/json",
                USER_AGENT,
                &request_id,
                get_session_id(),
                REDACTED_TOKEN,
            ),
            body: request_body,
        })
    } else {
        None
    };

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .header("x-request-id", &request_id)
        .header("x-request-session-id", get_session_id())
        .header("Authorization", format!("Bearer {}", config.token))
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let response_headers = if http_logger::is_enabled() {
                http_logger::extract_response_headers(&resp)
            } else {
                Vec::new()
            };
            let body_text = resp.text().await.unwrap_or_default();
            if let Some(ref req_log) = http_request_log {
                let response_log = HttpResponseLog {
                    status: status.as_u16(),
                    headers: response_headers,
                    body: Some(body_text.clone()),
                };
                http_logger::log_request(None, req_log, Some(&response_log), duration_ms, None);
            }
            handle_response_text(status.as_u16(), &body_text, true)
        }
        Err(e) => {
            let error_msg = e.to_string();
            if let Some(ref req_log) = http_request_log {
                http_logger::log_request(None, req_log, None, duration_ms, Some(&error_msg));
            }
            Err(anyhow!("Request failed: {}", error_msg))
        }
    }
}

/// Handle API response text
fn handle_response_text(status: u16, body_text: &str, is_old_endpoint: bool) -> Result<String> {
    if status == 401 {
        return Err(anyhow!("Token invalid or expired"));
    }
    if status == 403 {
        return Err(anyhow!("Access denied, token may be disabled"));
    }
    if !(200..300).contains(&status) {
        return Err(anyhow!(
            "Prompt enhancer API failed: {} - {}",
            status,
            body_text
        ));
    }

    let enhanced_text = if is_old_endpoint {
        parse_streaming_response(body_text)?
    } else {
        let resp: PromptEnhancerResponse = serde_json::from_str(body_text)
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;
        resp.text
            .ok_or_else(|| anyhow!("Prompt enhancer API returned empty result"))?
    };

    let enhanced_text = if is_old_endpoint {
        super::common::extract_enhanced_prompt(&enhanced_text)
            .unwrap_or_else(|| enhanced_text.clone())
    } else {
        enhanced_text
    };

    let enhanced_text = replace_tool_names(&enhanced_text);

    Ok(enhanced_text)
}

/// Parse streaming response from /chat-stream endpoint
/// Supports both raw JSON lines and SSE format (data: prefix)
pub fn parse_streaming_response(body_text: &str) -> Result<String> {
    let mut combined_text = String::new();
    let mut parsed_any = false;

    for line in body_text.lines() {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Handle SSE format: strip "data:" prefix
        if let Some(stripped) = line.strip_prefix("data:") {
            line = stripped.trim();
        }

        // Skip SSE termination marker
        if line.is_empty() || line == "[DONE]" {
            continue;
        }

        if let Ok(resp) = serde_json::from_str::<PromptEnhancerResponse>(line) {
            if let Some(text) = resp.text {
                combined_text.push_str(&text);
                parsed_any = true;
            }
        }
    }

    if parsed_any {
        Ok(combined_text)
    } else {
        let resp: PromptEnhancerResponse = serde_json::from_str(body_text)
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;
        resp.text
            .ok_or_else(|| anyhow!("Prompt enhancer API returned empty result"))
    }
}
