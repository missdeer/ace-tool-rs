//! Tests for third-party API endpoints (Claude, OpenAI, Gemini)
//! Uses wiremock to mock HTTP responses

use ace_tool::service::{
    call_claude_endpoint, call_gemini_endpoint, call_openai_endpoint, ThirdPartyConfig,
};
use reqwest::Client;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap()
}

// ============================================================================
// Claude API Tests
// ============================================================================

#[tokio::test]
async fn test_claude_api_success() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "<augment-enhanced-prompt>Enhanced prompt for testing</augment-enhanced-prompt>"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-token"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Enhanced prompt for testing");
}

#[tokio::test]
async fn test_claude_api_success_without_xml_tag() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "Plain enhanced prompt without XML tags"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Plain enhanced prompt without XML tags");
}

#[tokio::test]
async fn test_claude_api_multiple_content_blocks() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "First part "
            },
            {
                "type": "text",
                "text": "Second part"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "First part Second part");
}

#[tokio::test]
async fn test_claude_api_error_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "type": "authentication_error",
                "message": "Invalid API key"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "invalid-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Claude") && err.contains("invalid or expired"));
}

#[tokio::test]
async fn test_claude_api_empty_response() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 0
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty response"));
}

#[tokio::test]
async fn test_claude_api_with_conversation_history() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "Enhanced with history"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let history = "User: Hello\nAssistant: Hi there!";
    let result = call_claude_endpoint(&client, &config, "Test prompt", history).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Enhanced with history");
}

#[tokio::test]
async fn test_claude_api_url_normalization() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "Success"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    // URL with trailing /v1 should be normalized
    let config = ThirdPartyConfig {
        base_url: format!("{}/v1", mock_server.uri()),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
}

// ============================================================================
// OpenAI API Tests
// ============================================================================

#[tokio::test]
async fn test_openai_api_success() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "<augment-enhanced-prompt>OpenAI enhanced prompt</augment-enhanced-prompt>"
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-openai-token"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "OpenAI enhanced prompt");
}

#[tokio::test]
async fn test_openai_api_success_without_xml_tag() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Plain OpenAI response"
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Plain OpenAI response");
}

#[tokio::test]
async fn test_openai_api_error_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "message": "Incorrect API key provided",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "invalid-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("OpenAI") && err.contains("invalid or expired"));
}

#[tokio::test]
async fn test_openai_api_empty_choices() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 0,
            "total_tokens": 100
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty response"));
}

#[tokio::test]
async fn test_openai_api_with_conversation_history() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Response with history context"
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 150,
            "completion_tokens": 50,
            "total_tokens": 200
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let history = "User: What is Rust?\nAssistant: Rust is a systems programming language.";
    let result = call_openai_endpoint(&client, &config, "Tell me more", history).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Response with history context");
}

#[tokio::test]
async fn test_openai_api_url_normalization() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "chatcmpl-123456",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Success"
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    // URL with trailing /v1 should be normalized
    let config = ThirdPartyConfig {
        base_url: format!("{}/v1", mock_server.uri()),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
}

// ============================================================================
// Gemini API Tests
// ============================================================================

#[tokio::test]
async fn test_gemini_api_success() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "<augment-enhanced-prompt>Gemini enhanced prompt</augment-enhanced-prompt>"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 50,
            "totalTokenCount": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .and(header("x-goog-api-key", "test-gemini-token"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Gemini enhanced prompt");
}

#[tokio::test]
async fn test_gemini_api_success_without_xml_tag() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "Plain Gemini response"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 50,
            "totalTokenCount": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Plain Gemini response");
}

#[tokio::test]
async fn test_gemini_api_error_401() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "code": 401,
                "message": "API key not valid",
                "status": "UNAUTHENTICATED"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "invalid-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Gemini") && err.contains("invalid or expired"));
}

#[tokio::test]
async fn test_gemini_api_empty_candidates() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 0,
            "totalTokenCount": 100
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty response"));
}

#[tokio::test]
async fn test_gemini_api_with_conversation_history() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "Response considering history"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 150,
            "candidatesTokenCount": 50,
            "totalTokenCount": 200
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let history = "User: Hello\nAssistant: Hi!";
    let result = call_gemini_endpoint(&client, &config, "Continue", history).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Response considering history");
}

#[tokio::test]
async fn test_gemini_api_url_normalization() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "Success"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 50,
            "totalTokenCount": 150
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    // URL with trailing /v1beta should be normalized
    let config = ThirdPartyConfig {
        base_url: format!("{}/v1beta", mock_server.uri()),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_gemini_api_uses_header_not_query_param() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "Success"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 50,
            "totalTokenCount": 150
        }
    });

    // Verify x-goog-api-key header is used (security test)
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .and(header("x-goog-api-key", "secure-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "secure-api-key".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
}

// ============================================================================
// Tool Name Replacement Tests
// ============================================================================

#[tokio::test]
async fn test_claude_api_replaces_tool_names() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "id": "msg_01234567890",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "Use codebase-retrieval or codebase_retrieval tool"
            }
        ],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_ok());
    let text = result.unwrap();
    assert!(text.contains("search_context"));
    assert!(!text.contains("codebase-retrieval"));
    assert!(!text.contains("codebase_retrieval"));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_claude_api_rate_limit_429() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {
                "type": "rate_limit_error",
                "message": "Rate limit exceeded"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-token".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let result = call_claude_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("429"));
}

#[tokio::test]
async fn test_openai_api_server_error_500() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": {
                "message": "Internal server error",
                "type": "server_error"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-openai-token".to_string(),
        model: "gpt-4o".to_string(),
    };

    let result = call_openai_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("500"));
}

#[tokio::test]
async fn test_gemini_api_invalid_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash-exp:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client();
    let config = ThirdPartyConfig {
        base_url: mock_server.uri(),
        token: "test-gemini-token".to_string(),
        model: "gemini-2.0-flash-exp".to_string(),
    };

    let result = call_gemini_endpoint(&client, &config, "Test prompt", "").await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to parse Gemini response"));
}
