//! Claude API service

use std::time::Instant;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::common::{
    build_third_party_prompt, extract_enhanced_prompt, map_auth_error, parse_chat_history,
    replace_tool_names, ThirdPartyConfig,
};

/// Claude API request structure
#[derive(Debug, Serialize)]
struct ClaudeApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

/// Claude API response structure
#[derive(Debug, Deserialize)]
struct ClaudeApiResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

fn build_claude_url(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let base_url = base_url.strip_suffix("/v1").unwrap_or(base_url);
    format!("{}/v1/messages", base_url)
}

/// Call Claude API endpoint
pub async fn call_claude_endpoint(
    client: &Client,
    config: &ThirdPartyConfig,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let final_prompt = build_third_party_prompt(original_prompt)?;
    let chat_history = parse_chat_history(conversation_history);

    let mut messages: Vec<ClaudeMessage> = chat_history
        .into_iter()
        .map(|m| ClaudeMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    messages.push(ClaudeMessage {
        role: "user".to_string(),
        content: final_prompt,
    });

    let payload = ClaudeApiRequest {
        model: config.model.clone(),
        max_tokens: 4096,
        messages,
    };

    let url = build_claude_url(&config.base_url);
    let start_time = Instant::now();

    info!("Calling Claude API: {}", url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", &config.token)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    info!("Claude API call completed in {}ms", duration_ms);

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            if let Some(err) = map_auth_error(status.as_u16(), "Claude") {
                return Err(err);
            }

            if !status.is_success() {
                return Err(anyhow!("Claude API failed: {} - {}", status, body_text));
            }

            let api_response: ClaudeApiResponse = serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("Failed to parse Claude response: {} - {}", e, body_text))?;

            let text = api_response
                .content
                .into_iter()
                .filter(|c| c.content_type == "text")
                .filter_map(|c| c.text)
                .collect::<Vec<_>>()
                .join("");

            if text.is_empty() {
                return Err(anyhow!("Claude API returned empty response"));
            }

            let enhanced_text = extract_enhanced_prompt(&text).unwrap_or(text);
            let enhanced_text = replace_tool_names(&enhanced_text);

            Ok(enhanced_text)
        }
        Err(e) => Err(anyhow!("Claude API request failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_claude_url() {
        assert_eq!(
            build_claude_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_claude_url("https://api.anthropic.com/"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_claude_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_claude_url("https://api.anthropic.com/v1/"),
            "https://api.anthropic.com/v1/messages"
        );
    }
}
