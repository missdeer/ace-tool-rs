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
    pub id: String,
    pub enhanced_prompt: String,
    pub original_prompt: String,
    pub conversation_history: String,
    pub blob_names: Vec<String>,
    pub status: SessionStatus,
    pub created_at: Instant,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionStatus {
    Pending,
    Completed,
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
    pub timeout_ms: u64,
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
pub fn cors_response(mut response: Response<Full<Bytes>>) -> Response<Full<Bytes>> {
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
pub fn serve_enhancer_ui() -> Response<Full<Bytes>> {
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
pub fn json_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}
