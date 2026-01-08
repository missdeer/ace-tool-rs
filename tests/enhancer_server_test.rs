//! Tests for enhancer server module

use std::time::Instant;

use ace_tool::enhancer::server::{EnhancerServer, SessionData, SessionStatus};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};

// Import the internal functions we need to test
// These need to be made pub in the source file
use ace_tool::enhancer::server::{cors_response, json_response, serve_enhancer_ui};

// ========================================================================
// SessionStatus Tests
// ========================================================================

#[test]
fn test_session_status_equality() {
    assert_eq!(SessionStatus::Pending, SessionStatus::Pending);
    assert_eq!(SessionStatus::Completed, SessionStatus::Completed);
    assert_eq!(SessionStatus::Timeout, SessionStatus::Timeout);
    assert_ne!(SessionStatus::Pending, SessionStatus::Completed);
}

#[test]
fn test_session_status_clone() {
    let status = SessionStatus::Pending;
    let cloned = status.clone();
    assert_eq!(status, cloned);
}

#[test]
fn test_session_status_debug() {
    let pending = SessionStatus::Pending;
    let completed = SessionStatus::Completed;
    let timeout = SessionStatus::Timeout;

    assert_eq!(format!("{:?}", pending), "Pending");
    assert_eq!(format!("{:?}", completed), "Completed");
    assert_eq!(format!("{:?}", timeout), "Timeout");
}

#[test]
fn test_session_status_all_variants_different() {
    let variants = [
        SessionStatus::Pending,
        SessionStatus::Completed,
        SessionStatus::Timeout,
    ];

    for i in 0..variants.len() {
        for j in 0..variants.len() {
            if i == j {
                assert_eq!(variants[i], variants[j]);
            } else {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }
}

// ========================================================================
// SessionData Tests
// ========================================================================

#[test]
fn test_session_data_creation() {
    let data = SessionData {
        id: "test-id".to_string(),
        enhanced_prompt: "enhanced".to_string(),
        original_prompt: "original".to_string(),
        conversation_history: "history".to_string(),
        blob_names: vec!["blob1".to_string()],
        status: SessionStatus::Pending,
        created_at: Instant::now(),
        created_at_ms: 1234567890,
    };

    assert_eq!(data.id, "test-id");
    assert_eq!(data.enhanced_prompt, "enhanced");
    assert_eq!(data.original_prompt, "original");
    assert_eq!(data.conversation_history, "history");
    assert_eq!(data.blob_names.len(), 1);
    assert_eq!(data.status, SessionStatus::Pending);
}

#[test]
fn test_session_data_clone() {
    let data = SessionData {
        id: "test-id".to_string(),
        enhanced_prompt: "enhanced".to_string(),
        original_prompt: "original".to_string(),
        conversation_history: "history".to_string(),
        blob_names: vec!["blob1".to_string(), "blob2".to_string()],
        status: SessionStatus::Pending,
        created_at: Instant::now(),
        created_at_ms: 1234567890,
    };

    let cloned = data.clone();
    assert_eq!(cloned.id, data.id);
    assert_eq!(cloned.enhanced_prompt, data.enhanced_prompt);
    assert_eq!(cloned.original_prompt, data.original_prompt);
    assert_eq!(cloned.blob_names, data.blob_names);
    assert_eq!(cloned.status, data.status);
}

#[test]
fn test_session_data_with_empty_blobs() {
    let data = SessionData {
        id: "test".to_string(),
        enhanced_prompt: "enhanced".to_string(),
        original_prompt: "original".to_string(),
        conversation_history: "".to_string(),
        blob_names: vec![],
        status: SessionStatus::Pending,
        created_at: Instant::now(),
        created_at_ms: 0,
    };

    assert!(data.blob_names.is_empty());
}

#[test]
fn test_session_data_with_unicode() {
    let data = SessionData {
        id: "测试-id".to_string(),
        enhanced_prompt: "增强的提示".to_string(),
        original_prompt: "原始提示".to_string(),
        conversation_history: "用户: 你好\n助手: 你好！".to_string(),
        blob_names: vec!["文件.rs".to_string()],
        status: SessionStatus::Pending,
        created_at: Instant::now(),
        created_at_ms: 1234567890,
    };

    assert_eq!(data.enhanced_prompt, "增强的提示");
    assert!(data.conversation_history.contains("你好"));
}

// ========================================================================
// EnhancerServer Tests
// ========================================================================

#[test]
fn test_enhancer_server_new() {
    let _server = EnhancerServer::new();
    // Server should be created without panicking
}

#[test]
fn test_enhancer_server_default() {
    let _server = EnhancerServer::default();
    // Default should work the same as new()
}

#[tokio::test]
async fn test_enhancer_server_get_port_default() {
    let server = EnhancerServer::new();
    let port = server.get_port().await;
    assert_eq!(port, 3000);
}

#[tokio::test]
async fn test_enhancer_server_create_session() {
    let server = EnhancerServer::new();
    let (session_id, _rx) = server
        .create_session(
            "enhanced".to_string(),
            "original".to_string(),
            "history".to_string(),
            vec!["blob".to_string()],
        )
        .await;

    // Session ID should be a valid UUID
    assert!(!session_id.is_empty());
    assert!(session_id.contains('-')); // UUIDs contain hyphens
}

#[tokio::test]
async fn test_enhancer_server_create_multiple_sessions() {
    let server = EnhancerServer::new();

    let (id1, _rx1) = server
        .create_session(
            "enhanced1".to_string(),
            "original1".to_string(),
            "history1".to_string(),
            vec![],
        )
        .await;

    let (id2, _rx2) = server
        .create_session(
            "enhanced2".to_string(),
            "original2".to_string(),
            "history2".to_string(),
            vec![],
        )
        .await;

    // Each session should have a unique ID
    assert_ne!(id1, id2);
}

#[tokio::test]
async fn test_enhancer_server_create_session_with_empty_data() {
    let server = EnhancerServer::new();
    let (session_id, _rx) = server
        .create_session("".to_string(), "".to_string(), "".to_string(), vec![])
        .await;

    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn test_enhancer_server_create_session_with_large_data() {
    let server = EnhancerServer::new();
    let large_prompt = "x".repeat(100000);
    let many_blobs: Vec<String> = (0..1000).map(|i| format!("blob_{}", i)).collect();

    let (session_id, _rx) = server
        .create_session(
            large_prompt.clone(),
            large_prompt,
            "history".to_string(),
            many_blobs,
        )
        .await;

    assert!(!session_id.is_empty());
}

// ========================================================================
// JSON Response Helper Tests
// ========================================================================

#[test]
fn test_json_response_ok() {
    let response = json_response(StatusCode::OK, r#"{"success":true}"#);
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_json_response_bad_request() {
    let response = json_response(StatusCode::BAD_REQUEST, r#"{"error":"bad"}"#);
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_json_response_not_found() {
    let response = json_response(StatusCode::NOT_FOUND, r#"{"error":"not found"}"#);
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_json_response_internal_error() {
    let response = json_response(StatusCode::INTERNAL_SERVER_ERROR, r#"{"error":"internal"}"#);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_json_response_content_type() {
    let response = json_response(StatusCode::OK, "{}");
    let content_type = response.headers().get("Content-Type").unwrap();
    assert_eq!(content_type, "application/json");
}

#[test]
fn test_json_response_empty_body() {
    let response = json_response(StatusCode::OK, "");
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_json_response_with_unicode() {
    let response = json_response(StatusCode::OK, r#"{"message":"你好世界"}"#);
    assert_eq!(response.status(), StatusCode::OK);
}

// ========================================================================
// CORS Response Tests
// ========================================================================

#[test]
fn test_cors_response_adds_headers() {
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let cors_resp = cors_response(response);

    assert!(cors_resp
        .headers()
        .contains_key("Access-Control-Allow-Origin"));
    assert!(cors_resp
        .headers()
        .contains_key("Access-Control-Allow-Methods"));
    assert!(cors_resp
        .headers()
        .contains_key("Access-Control-Allow-Headers"));
}

#[test]
fn test_cors_response_allows_localhost_origin() {
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let cors_resp = cors_response(response);
    let origin = cors_resp
        .headers()
        .get("Access-Control-Allow-Origin")
        .unwrap();
    assert_eq!(origin, "http://localhost");
}

#[test]
fn test_cors_response_allows_required_methods() {
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let cors_resp = cors_response(response);
    let methods = cors_resp
        .headers()
        .get("Access-Control-Allow-Methods")
        .unwrap();
    let methods_str = methods.to_str().unwrap();

    assert!(methods_str.contains("GET"));
    assert!(methods_str.contains("POST"));
    assert!(methods_str.contains("OPTIONS"));
}

#[test]
fn test_cors_response_preserves_status() {
    let response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let cors_resp = cors_response(response);
    assert_eq!(cors_resp.status(), StatusCode::NOT_FOUND);
}

// ========================================================================
// serve_enhancer_ui Tests
// ========================================================================

#[test]
fn test_serve_enhancer_ui_returns_ok() {
    let response = serve_enhancer_ui();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_serve_enhancer_ui_content_type() {
    let response = serve_enhancer_ui();
    let content_type = response.headers().get("Content-Type").unwrap();
    assert!(content_type.to_str().unwrap().contains("text/html"));
    assert!(content_type.to_str().unwrap().contains("utf-8"));
}

// ========================================================================
// Timeout Configuration Tests
// ========================================================================

#[test]
fn test_timeout_is_8_minutes() {
    let server = EnhancerServer::new();
    // The timeout is 8 * 60 * 1000 = 480000 ms
    assert_eq!(server.timeout_ms, 8 * 60 * 1000);
}
