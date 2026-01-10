//! Gemini API service

use std::time::Instant;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::common::{
    build_third_party_prompt, extract_enhanced_prompt, map_auth_error, parse_chat_history,
    replace_tool_names, ThirdPartyConfig,
};

/// Gemini API request structure
#[derive(Debug, Serialize)]
struct GeminiApiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

/// Gemini API response structure
#[derive(Debug, Deserialize)]
struct GeminiApiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

fn build_gemini_url(base_url: &str, model: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let base_url = base_url.strip_suffix("/v1beta").unwrap_or(base_url);
    format!("{}/v1beta/models/{}:generateContent", base_url, model)
}

/// Call Gemini API endpoint
pub async fn call_gemini_endpoint(
    client: &Client,
    config: &ThirdPartyConfig,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let final_prompt = build_third_party_prompt(original_prompt)?;
    let chat_history = parse_chat_history(conversation_history);

    let mut contents: Vec<GeminiContent> = chat_history
        .into_iter()
        .map(|m| {
            let role = if m.role == "assistant" {
                "model"
            } else {
                "user"
            };
            GeminiContent {
                role: role.to_string(),
                parts: vec![GeminiPart { text: m.content }],
            }
        })
        .collect();

    contents.push(GeminiContent {
        role: "user".to_string(),
        parts: vec![GeminiPart { text: final_prompt }],
    });

    let payload = GeminiApiRequest {
        contents,
        generation_config: Some(GeminiGenerationConfig {
            max_output_tokens: 4096,
        }),
    };

    let url = build_gemini_url(&config.base_url, &config.model);
    let start_time = Instant::now();

    info!("Calling Gemini API: {}", url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", &config.token)
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    info!("Gemini API call completed in {}ms", duration_ms);

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            if let Some(err) = map_auth_error(status.as_u16(), "Gemini") {
                return Err(err);
            }

            if !status.is_success() {
                return Err(anyhow!("Gemini API failed: {} - {}", status, body_text));
            }

            let api_response: GeminiApiResponse = serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("Failed to parse Gemini response: {} - {}", e, body_text))?;

            let text = api_response
                .candidates
                .first()
                .and_then(|c| c.content.parts.first())
                .and_then(|p| p.text.clone())
                .ok_or_else(|| anyhow!("Gemini API returned empty response"))?;

            let enhanced_text = extract_enhanced_prompt(&text).unwrap_or(text);
            let enhanced_text = replace_tool_names(&enhanced_text);

            Ok(enhanced_text)
        }
        Err(e) => Err(anyhow!("Gemini API request failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gemini_url() {
        assert_eq!(
            build_gemini_url("https://generativelanguage.googleapis.com", "gemini-pro"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
        assert_eq!(
            build_gemini_url("https://generativelanguage.googleapis.com/", "gemini-pro"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
        assert_eq!(
            build_gemini_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "gemini-pro"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
        assert_eq!(
            build_gemini_url(
                "https://generativelanguage.googleapis.com/v1beta/",
                "gemini-pro"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
    }
}
