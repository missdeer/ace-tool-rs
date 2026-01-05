//! Enhancer Server - HTTP server and Session management
//! Provides Web UI interaction interface

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::templates::ENHANCER_UI_HTML;

/// Maximum request body size (1MB)
const MAX_BODY_SIZE: usize = 1024 * 1024;

/// Callback type for re-enhancement
pub type EnhanceCallback = Arc<
    dyn Fn(
            String,
            String,
            Vec<String>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        + Send
        + Sync,
>;

/// Session data structure
#[derive(Clone)]
pub struct SessionData {
    #[allow(dead_code)]
    pub id: String,
    pub enhanced_prompt: String,
    pub original_prompt: String,
    pub conversation_history: String,
    pub blob_names: Vec<String>,
    pub status: SessionStatus,
    #[allow(dead_code)]
    pub created_at: Instant,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionStatus {
    Pending,
    Completed,
    #[allow(dead_code)]
    Timeout,
}

/// Session response sender
struct SessionResponder {
    sender: oneshot::Sender<String>,
}

/// Enhancer HTTP Server
pub struct EnhancerServer {
    port: Arc<RwLock<u16>>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    responders: Arc<Mutex<HashMap<String, SessionResponder>>>,
    enhance_callback: Arc<RwLock<Option<EnhanceCallback>>>,
    running: Arc<RwLock<bool>>,
    timeout_ms: u64,
}

impl EnhancerServer {
    pub fn new() -> Self {
        Self {
            port: Arc::new(RwLock::new(3000)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            responders: Arc::new(Mutex::new(HashMap::new())),
            enhance_callback: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
            timeout_ms: 8 * 60 * 1000, // 8 minutes
        }
    }

    /// Start HTTP server
    pub async fn start(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                return Ok(()); // Already running
            }
            *running = true;
        }

        let mut port = *self.port.read().await;
        let mut listener: Option<TcpListener> = None;

        // Try to bind to port, increment if in use
        for _ in 0..100 {
            match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await {
                Ok(l) => {
                    listener = Some(l);
                    break;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        warn!("Port {} is in use, trying {}", port, port + 1);
                        port += 1;
                    } else {
                        let mut running = self.running.write().await;
                        *running = false;
                        return Err(anyhow!("Failed to bind to port: {}", e));
                    }
                }
            }
        }

        let listener = match listener {
            Some(l) => l,
            None => {
                let mut running = self.running.write().await;
                *running = false;
                return Err(anyhow!("Could not find available port"));
            }
        };

        {
            let mut port_lock = self.port.write().await;
            *port_lock = port;
        }

        info!("Enhancer server started: http://localhost:{}", port);

        // Clone references for the server task
        let sessions = self.sessions.clone();
        let responders = self.responders.clone();
        let enhance_callback = self.enhance_callback.clone();
        let timeout_ms = self.timeout_ms;

        // Spawn server task
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        continue;
                    }
                };

                let io = TokioIo::new(stream);
                let sessions = sessions.clone();
                let responders = responders.clone();
                let enhance_callback = enhance_callback.clone();

                tokio::spawn(async move {
                    let service = service_fn(|req| {
                        let sessions = sessions.clone();
                        let responders = responders.clone();
                        let enhance_callback = enhance_callback.clone();
                        async move {
                            handle_request(req, sessions, responders, enhance_callback, timeout_ms)
                                .await
                        }
                    });

                    if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                        if !e.to_string().contains("connection closed") {
                            error!("Error serving connection: {}", e);
                        }
                    }
                });
            }
        });

        Ok(())
    }

    /// Get server port
    pub async fn get_port(&self) -> u16 {
        *self.port.read().await
    }

    /// Create new session and return a receiver for the result
    /// The responder is registered at creation time to prevent race conditions
    pub async fn create_session(
        &self,
        enhanced_prompt: String,
        original_prompt: String,
        conversation_history: String,
        blob_names: Vec<String>,
    ) -> (String, oneshot::Receiver<String>) {
        let session_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        let created_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let session = SessionData {
            id: session_id.clone(),
            enhanced_prompt,
            original_prompt,
            conversation_history,
            blob_names,
            status: SessionStatus::Pending,
            created_at: now,
            created_at_ms,
        };

        // Create oneshot channel and register responder BEFORE inserting session
        // This prevents race condition where submit arrives before wait_for_session
        let (tx, rx) = oneshot::channel();

        {
            let mut responders = self.responders.lock().await;
            responders.insert(session_id.clone(), SessionResponder { sender: tx });
        }

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        info!("Created session: {}", session_id);
        (session_id, rx)
    }

    /// Wait for session completion using a pre-created receiver
    pub async fn wait_for_session_with_receiver(
        &self,
        session_id: &str,
        rx: oneshot::Receiver<String>,
    ) -> Result<String> {
        // Set up timeout
        let timeout = Duration::from_millis(self.timeout_ms);

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => {
                // Clean up session
                {
                    let mut sessions = self.sessions.write().await;
                    sessions.remove(session_id);
                }
                Ok(result)
            }
            Ok(Err(_)) => {
                // Channel closed - clean up session
                {
                    let mut sessions = self.sessions.write().await;
                    sessions.remove(session_id);
                }
                Err(anyhow!("Session channel closed unexpectedly"))
            }
            Err(_) => {
                // Timeout - clean up session and responder
                {
                    let mut sessions = self.sessions.write().await;
                    sessions.remove(session_id);
                }
                {
                    let mut responders = self.responders.lock().await;
                    responders.remove(session_id);
                }
                Err(anyhow!("User interaction timeout (8 minutes)"))
            }
        }
    }

    /// Set enhance callback
    pub async fn set_enhance_callback(&self, callback: EnhanceCallback) {
        let mut cb = self.enhance_callback.write().await;
        *cb = Some(callback);
    }
}

impl Default for EnhancerServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle HTTP request
async fn handle_request(
    req: Request<Incoming>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    responders: Arc<Mutex<HashMap<String, SessionResponder>>>,
    enhance_callback: Arc<RwLock<Option<EnhanceCallback>>>,
    timeout_ms: u64,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|s| s.to_string());

    // Handle CORS preflight
    if method == Method::OPTIONS {
        return Ok(cors_response(
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::new()))
                .unwrap(),
        ));
    }

    let response = match (method, path.as_str()) {
        (Method::GET, "/enhance") => serve_enhancer_ui(),
        (Method::GET, "/api/session") => get_session_data(query, sessions, timeout_ms).await,
        (Method::POST, "/api/submit") => handle_submit(req, sessions, responders).await,
        (Method::POST, "/api/re-enhance") => {
            handle_re_enhance(req, sessions, enhance_callback).await
        }
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain")
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap(),
    };

    Ok(cors_response(response))
}

/// Add CORS headers (restricted to localhost only)
fn cors_response(mut response: Response<Full<Bytes>>) -> Response<Full<Bytes>> {
    let headers = response.headers_mut();
    headers.insert(
        "Access-Control-Allow-Origin",
        "http://localhost".parse().unwrap(),
    );
    headers.insert(
        "Access-Control-Allow-Methods",
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        "Content-Type".parse().unwrap(),
    );
    response
}

/// Serve Web UI HTML
fn serve_enhancer_ui() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(ENHANCER_UI_HTML)))
        .unwrap()
}

/// Get session data
async fn get_session_data(
    query: Option<String>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    timeout_ms: u64,
) -> Response<Full<Bytes>> {
    let session_id = query.and_then(|q| {
        q.split('&').find_map(|param| {
            let mut parts = param.splitn(2, '=');
            if parts.next()? == "session" {
                Some(parts.next()?.to_string())
            } else {
                None
            }
        })
    });

    let session_id = match session_id {
        Some(id) => id,
        None => {
            return json_response(
                StatusCode::BAD_REQUEST,
                r#"{"error":"Session ID is required"}"#,
            );
        }
    };

    let sessions = sessions.read().await;
    let session = match sessions.get(&session_id) {
        Some(s) => s,
        None => {
            return json_response(StatusCode::NOT_FOUND, r#"{"error":"Session not found"}"#);
        }
    };

    #[derive(Serialize)]
    struct SessionResponse {
        #[serde(rename = "enhancedPrompt")]
        enhanced_prompt: String,
        status: String,
        #[serde(rename = "createdAt")]
        created_at: u64,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u64,
    }

    let resp = SessionResponse {
        enhanced_prompt: session.enhanced_prompt.clone(),
        status: match session.status {
            SessionStatus::Pending => "pending",
            SessionStatus::Completed => "completed",
            SessionStatus::Timeout => "timeout",
        }
        .to_string(),
        created_at: session.created_at_ms,
        timeout_ms,
    };

    json_response(StatusCode::OK, &serde_json::to_string(&resp).unwrap())
}

/// Handle user submit
async fn handle_submit(
    req: Request<Incoming>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    responders: Arc<Mutex<HashMap<String, SessionResponder>>>,
) -> Response<Full<Bytes>> {
    let body = match read_body_with_limit(req, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            return json_error_response(StatusCode::BAD_REQUEST, &e);
        }
    };

    #[derive(Deserialize)]
    struct SubmitRequest {
        #[serde(rename = "sessionId")]
        session_id: String,
        content: String,
        #[serde(default)]
        action: Option<String>,
    }

    let submit: SubmitRequest = match serde_json::from_slice(&body) {
        Ok(s) => s,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "Invalid request body");
        }
    };

    // Get session and update status
    let original_prompt = {
        let mut sessions = sessions.write().await;
        let session = match sessions.get_mut(&submit.session_id) {
            Some(s) => s,
            None => {
                return json_error_response(StatusCode::NOT_FOUND, "Session not found");
            }
        };

        if session.status != SessionStatus::Pending {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "Session already completed or timed out",
            );
        }

        session.status = SessionStatus::Completed;
        session.original_prompt.clone()
    };

    // Determine what to send back - check action field first, then fallback to magic strings
    let result = match submit.action.as_deref() {
        Some("use_original") => {
            info!("User chose to use original prompt");
            original_prompt
        }
        Some("end_conversation") => {
            info!("User chose to end conversation");
            "__END_CONVERSATION__".to_string()
        }
        _ => {
            // Fallback to magic strings for backward compatibility
            if submit.content == "__USE_ORIGINAL__" {
                info!("User chose to use original prompt");
                original_prompt
            } else if submit.content == "__END_CONVERSATION__" {
                info!("User chose to end conversation");
                "__END_CONVERSATION__".to_string()
            } else {
                submit.content
            }
        }
    };

    // Send result through channel
    {
        let mut responders = responders.lock().await;
        if let Some(responder) = responders.remove(&submit.session_id) {
            let _ = responder.sender.send(result);
        }
    }

    info!("Session {} completed", submit.session_id);
    json_response(
        StatusCode::OK,
        &serde_json::to_string(&json!({"success": true})).unwrap(),
    )
}

/// Handle re-enhance request
async fn handle_re_enhance(
    req: Request<Incoming>,
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    enhance_callback: Arc<RwLock<Option<EnhanceCallback>>>,
) -> Response<Full<Bytes>> {
    let body = match read_body_with_limit(req, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            return json_error_response(StatusCode::BAD_REQUEST, &e);
        }
    };

    #[derive(Deserialize)]
    struct ReEnhanceRequest {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "currentPrompt")]
        current_prompt: String,
    }

    let req_data: ReEnhanceRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "Invalid request body");
        }
    };

    // Get session data
    let (conversation_history, blob_names, status) = {
        let sessions = sessions.read().await;
        let session = match sessions.get(&req_data.session_id) {
            Some(s) => s,
            None => {
                return json_error_response(StatusCode::NOT_FOUND, "Session not found");
            }
        };
        (
            session.conversation_history.clone(),
            session.blob_names.clone(),
            session.status.clone(),
        )
    };

    if status != SessionStatus::Pending {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "Session already completed or timed out",
        );
    }

    // Get callback
    let callback = {
        let cb = enhance_callback.read().await;
        cb.clone()
    };

    let callback = match callback {
        Some(cb) => cb,
        None => {
            return json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Enhance callback not configured",
            );
        }
    };

    info!("Re-enhancing session: {}", req_data.session_id);

    // Call enhance callback
    match callback(req_data.current_prompt, conversation_history, blob_names).await {
        Ok(enhanced) => {
            // Update session
            {
                let mut sessions = sessions.write().await;
                if let Some(session) = sessions.get_mut(&req_data.session_id) {
                    session.enhanced_prompt = enhanced.clone();
                }
            }

            json_response(
                StatusCode::OK,
                &serde_json::to_string(&json!({"enhancedPrompt": enhanced})).unwrap(),
            )
        }
        Err(e) => {
            error!("Re-enhancement failed: {}", e);
            json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Enhancement failed: {}", e),
            )
        }
    }
}

/// Read request body with size limit (streaming enforcement to prevent memory exhaustion)
async fn read_body_with_limit(req: Request<Incoming>, max_size: usize) -> Result<Bytes, String> {
    let limited = Limited::new(req.into_body(), max_size);
    match limited.collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("length limit exceeded") {
                Err(format!("Request body too large (max {} bytes)", max_size))
            } else {
                Err("Failed to read body".to_string())
            }
        }
    }
}

/// Create JSON error response with safe serialization
fn json_error_response(status: StatusCode, error: &str) -> Response<Full<Bytes>> {
    let body = serde_json::to_string(&json!({"error": error})).unwrap();
    json_response(status, &body)
}

/// Create JSON response
fn json_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
