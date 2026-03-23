//! Codex API service (OpenAI Responses API)

use std::time::Instant;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use super::common::{
    build_third_party_prompt, extract_enhanced_prompt, map_auth_error, parse_chat_history,
    replace_tool_names, ThirdPartyConfig,
};

/// Codex API request structure (OpenAI Responses API)
#[derive(Debug, Serialize)]
struct CodexApiRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

/// Codex API response structure
#[derive(Debug, Deserialize)]
struct CodexApiResponse {
    output: Vec<CodexOutput>,
}

#[derive(Debug, Deserialize)]
struct CodexOutput {
    #[serde(rename = "type")]
    output_type: String,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    content: Option<Vec<ContentPart>>,
}

#[derive(Debug, Deserialize)]
struct ContentPart {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    refusal: Option<String>,
}

fn build_codex_url(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let base_url = base_url.strip_suffix("/v1").unwrap_or(base_url);
    format!("{}/v1/responses", base_url)
}

/// Build input message with role-aware content structure
fn build_input_message(role: &str, content: &str) -> Value {
    let content_type = match role {
        "assistant" => "output_text",
        _ => "input_text",
    };

    serde_json::json!({
        "type": "message",
        "role": role,
        "content": [{
            "type": content_type,
            "text": content
        }]
    })
}

/// Extract output text from Codex API response
/// Prioritizes final_answer phase over commentary, aggregates multiple output_text parts,
/// and surfaces refusal responses clearly
fn extract_output_text(api_response: &CodexApiResponse) -> Result<String> {
    // First try to find final_answer phase messages
    let final_answer_messages: Vec<&CodexOutput> = api_response
        .output
        .iter()
        .filter(|o| o.output_type == "message" && o.phase.as_deref() == Some("final_answer"))
        .collect();

    // If no final_answer phase, use all message outputs
    let candidate_messages: Vec<&CodexOutput> = if final_answer_messages.is_empty() {
        api_response
            .output
            .iter()
            .filter(|o| o.output_type == "message")
            .collect()
    } else {
        final_answer_messages
    };

    let mut output_text_parts = Vec::new();
    let mut refusal_parts = Vec::new();

    for msg in candidate_messages {
        let Some(parts) = msg.content.as_ref() else {
            continue;
        };

        for part in parts {
            match part.content_type.as_str() {
                "output_text" => {
                    if let Some(text) = part
                        .text
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        output_text_parts.push(text.to_string());
                    }
                }
                "refusal" => {
                    if let Some(refusal) = part
                        .refusal
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        refusal_parts.push(refusal.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    if !output_text_parts.is_empty() {
        return Ok(output_text_parts.join("\n"));
    }
    if !refusal_parts.is_empty() {
        return Err(anyhow!("Codex API refusal: {}", refusal_parts.join("\n")));
    }
    Err(anyhow!("Codex API returned no output_text content"))
}

/// Call Codex API endpoint (OpenAI Responses API)
pub async fn call_codex_endpoint(
    client: &Client,
    config: &ThirdPartyConfig,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let final_prompt = build_third_party_prompt(original_prompt)?;
    let chat_history = parse_chat_history(conversation_history);

    // Build input as array of message objects with explicit structure
    let mut messages: Vec<Value> = chat_history
        .into_iter()
        .map(|m| build_input_message(&m.role, &m.content))
        .collect();

    messages.push(build_input_message("user", &final_prompt));

    let payload = CodexApiRequest {
        model: config.model.clone(),
        input: Some(Value::Array(messages)),
        instructions: None,
        max_output_tokens: Some(4096),
    };

    let url = build_codex_url(&config.base_url);
    let start_time = Instant::now();

    info!("Calling Codex API: {}", url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.token))
        .json(&payload)
        .send()
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    info!("Codex API call completed in {}ms", duration_ms);

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            if let Some(err) = map_auth_error(status.as_u16(), "Codex") {
                return Err(err);
            }

            if !status.is_success() {
                return Err(anyhow!("Codex API failed: {} - {}", status, body_text));
            }

            let api_response: CodexApiResponse = serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("Failed to parse Codex response: {} - {}", e, body_text))?;

            let text = extract_output_text(&api_response)?;

            let enhanced_text = extract_enhanced_prompt(&text).unwrap_or(text);
            let enhanced_text = replace_tool_names(&enhanced_text);

            Ok(enhanced_text)
        }
        Err(e) => Err(anyhow!("Codex API request failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_codex_url() {
        assert_eq!(
            build_codex_url("https://api.openai.com"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            build_codex_url("https://api.openai.com/"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            build_codex_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            build_codex_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn test_build_input_message_uses_output_text_for_assistant_role() {
        let assistant_message = build_input_message("assistant", "Hello");
        assert_eq!(assistant_message["content"][0]["type"], "output_text");

        let user_message = build_input_message("user", "Hello");
        assert_eq!(user_message["content"][0]["type"], "input_text");
    }

    #[test]
    fn test_extract_output_text_prefers_final_answer_phase() {
        let api_response = CodexApiResponse {
            output: vec![
                CodexOutput {
                    output_type: "message".to_string(),
                    phase: Some("commentary".to_string()),
                    content: Some(vec![ContentPart {
                        content_type: "output_text".to_string(),
                        text: Some("intermediate".to_string()),
                        refusal: None,
                    }]),
                },
                CodexOutput {
                    output_type: "message".to_string(),
                    phase: Some("final_answer".to_string()),
                    content: Some(vec![ContentPart {
                        content_type: "output_text".to_string(),
                        text: Some("final".to_string()),
                        refusal: None,
                    }]),
                },
            ],
        };
        assert_eq!(extract_output_text(&api_response).unwrap(), "final");
    }

    #[test]
    fn test_extract_output_text_joins_multiple_parts() {
        let api_response = CodexApiResponse {
            output: vec![CodexOutput {
                output_type: "message".to_string(),
                phase: None,
                content: Some(vec![
                    ContentPart {
                        content_type: "output_text".to_string(),
                        text: Some("part 1".to_string()),
                        refusal: None,
                    },
                    ContentPart {
                        content_type: "output_text".to_string(),
                        text: Some("part 2".to_string()),
                        refusal: None,
                    },
                ]),
            }],
        };
        assert_eq!(
            extract_output_text(&api_response).unwrap(),
            "part 1\npart 2"
        );
    }

    #[test]
    fn test_extract_output_text_reports_refusal() {
        let api_response = CodexApiResponse {
            output: vec![CodexOutput {
                output_type: "message".to_string(),
                phase: None,
                content: Some(vec![ContentPart {
                    content_type: "refusal".to_string(),
                    text: None,
                    refusal: Some("safety refusal".to_string()),
                }]),
            }],
        };
        assert!(extract_output_text(&api_response)
            .unwrap_err()
            .to_string()
            .contains("safety refusal"));
    }
}
