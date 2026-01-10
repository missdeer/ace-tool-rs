//! OpenAI API service

use std::time::Instant;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::common::{
    build_third_party_prompt, extract_enhanced_prompt, map_auth_error, parse_chat_history,
    replace_tool_names, ThirdPartyConfig,
};

/// OpenAI API request structure
#[derive(Debug, Serialize)]
struct OpenAIApiRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

/// OpenAI API response structure
#[derive(Debug, Deserialize)]
struct OpenAIApiResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
}

fn build_openai_url(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let base_url = base_url.strip_suffix("/v1").unwrap_or(base_url);
    format!("{}/v1/chat/completions", base_url)
}

/// Call OpenAI API endpoint
pub async fn call_openai_endpoint(
    client: &Client,
    config: &ThirdPartyConfig,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let final_prompt = build_third_party_prompt(original_prompt)?;
    let chat_history = parse_chat_history(conversation_history);

    let mut messages: Vec<OpenAIMessage> = chat_history
        .into_iter()
        .map(|m| OpenAIMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    messages.push(OpenAIMessage {
        role: "user".to_string(),
        content: final_prompt,
    });

    let payload = OpenAIApiRequest {
        model: config.model.clone(),
        messages,
        max_tokens: Some(4096),
    };

    let url = build_openai_url(&config.base_url);
    let start_time = Instant::now();

    info!("Calling OpenAI API: {}", url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.token))
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    info!("OpenAI API call completed in {}ms", duration_ms);

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            if let Some(err) = map_auth_error(status.as_u16(), "OpenAI") {
                return Err(err);
            }

            if !status.is_success() {
                return Err(anyhow!("OpenAI API failed: {} - {}", status, body_text));
            }

            let api_response: OpenAIApiResponse = serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("Failed to parse OpenAI response: {} - {}", e, body_text))?;

            let text = api_response
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .ok_or_else(|| anyhow!("OpenAI API returned empty response"))?;

            let enhanced_text = extract_enhanced_prompt(&text).unwrap_or(text);
            let enhanced_text = replace_tool_names(&enhanced_text);

            Ok(enhanced_text)
        }
        Err(e) => Err(anyhow!("OpenAI API request failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_openai_url() {
        assert_eq!(
            build_openai_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("https://api.openai.com/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }
}
