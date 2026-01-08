//! Prompt Enhancer - Core enhancement logic
//! Based on Augment VSCode plugin implementation
//!
//! Supports two API endpoints controlled by environment variable `ACE_ENHANCER_ENDPOINT`:
//! - `new`: Uses `/prompt-enhancer` endpoint (simplified request, matches augment.mjs)
//! - `old` (default): Uses `/chat-stream` endpoint (full request with blobs)

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::http_logger::{self, HttpRequestLog, HttpResponseLog};
use crate::utils::project_detector::get_index_file_path;

use super::server::EnhancerServer;
use super::templates::ENHANCE_PROMPT_TEMPLATE;

/// Default model for prompt enhancement API
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5";

/// Environment variable to control which endpoint to use
/// Values: "new" (prompt-enhancer endpoint, default) or "old" (chat-stream endpoint)
pub const ENV_ENHANCER_ENDPOINT: &str = "ACE_ENHANCER_ENDPOINT";

/// Node ID for NEW endpoint (matches augment.mjs promptEnhancer)
pub const NODE_ID_NEW: i32 = 0;

/// Node ID for OLD endpoint
pub const NODE_ID_OLD: i32 = 1;

/// User-Agent header value (matches augment.mjs format: augment.cli/{version}/{mode})
const USER_AGENT: &str = "augment.cli/0.12.0/mcp";

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

/// Check if we should use the new prompt-enhancer endpoint (default: true)
pub fn use_new_endpoint() -> bool {
    std::env::var(ENV_ENHANCER_ENDPOINT)
        .map(|v| !v.trim().eq_ignore_ascii_case("old"))
        .unwrap_or(true)
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
/// Matches augment.mjs chatStream request structure
/// Note: All Option fields serialize as null (not skipped) to match augment.mjs behavior
/// where undefined values are converted to null via dG() function
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Response from prompt-enhancer API
#[derive(Debug, Deserialize)]
struct PromptEnhancerResponse {
    text: Option<String>,
}

/// Prompt Enhancer
pub struct PromptEnhancer {
    config: Arc<Config>,
    client: Client,
    server: Arc<EnhancerServer>,
}

impl PromptEnhancer {
    /// Create a new PromptEnhancer
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

        let server = Arc::new(EnhancerServer::new());

        Ok(Self {
            config,
            client,
            server,
        })
    }

    /// Enhance a prompt with codebase context and conversation history
    ///
    /// # Arguments
    /// * `original_prompt` - The original user input
    /// * `conversation_history` - Conversation history (5-10 rounds)
    /// * `project_root` - Project root path (optional, for loading blob names)
    ///
    /// # Returns
    /// Enhanced prompt text
    pub async fn enhance(
        &self,
        original_prompt: &str,
        conversation_history: &str,
        project_root: Option<&Path>,
    ) -> Result<String> {
        info!("Starting prompt enhancement...");

        // Load blob names if project root is provided
        let blob_names = if let Some(root) = project_root {
            self.load_blob_names(root)
        } else {
            Vec::new()
        };

        if blob_names.is_empty() {
            warn!("No index data found, enhancing without code context");
        } else {
            info!("Loaded {} file chunks", blob_names.len());
        }

        // Set up enhance callback for re-enhancement
        let config = self.config.clone();
        let client = self.client.clone();
        let callback = Arc::new(move |prompt: String, history: String, blobs: Vec<String>| {
            let config = config.clone();
            let client = client.clone();
            Box::pin(async move {
                call_prompt_enhancer_api_static(&client, &config, &prompt, &history, &blobs).await
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        });
        self.server.set_enhance_callback(callback).await;

        // Call prompt-enhancer API
        info!("Calling prompt-enhancer API...");
        let enhanced_prompt = self
            .call_prompt_enhancer_api(original_prompt, conversation_history, &blob_names)
            .await?;
        info!("Enhancement complete");

        // Start Web UI interaction
        info!("Starting Web UI for user review...");
        let final_prompt = self
            .interact_with_user(
                &enhanced_prompt,
                original_prompt,
                conversation_history,
                &blob_names,
            )
            .await?;

        info!("Prompt enhancement complete");
        Ok(final_prompt)
    }

    /// Interact with user through Web UI
    async fn interact_with_user(
        &self,
        enhanced_prompt: &str,
        original_prompt: &str,
        conversation_history: &str,
        blob_names: &[String],
    ) -> Result<String> {
        // Start server
        self.server.start().await?;

        // Create session (responder is registered at creation time to prevent race conditions)
        let (session_id, rx) = self
            .server
            .create_session(
                enhanced_prompt.to_string(),
                original_prompt.to_string(),
                conversation_history.to_string(),
                blob_names.to_vec(),
            )
            .await;

        // Build URL
        let port = self.server.get_port().await;
        let url = format!("http://localhost:{}/enhance?session={}", port, session_id);
        info!("Please open in browser: {}", url);

        // Try to open browser
        self.open_browser(&url);

        // Wait for user action using the pre-created receiver
        match self
            .server
            .wait_for_session_with_receiver(&session_id, rx)
            .await
        {
            Ok(result) => {
                if result.is_empty() {
                    Err(anyhow!("User cancelled the enhancement"))
                } else {
                    Ok(result)
                }
            }
            Err(e) => {
                if e.to_string().contains("timeout") {
                    error!("User interaction timeout (8 minutes)");
                }
                Err(e)
            }
        }
    }

    /// Open browser
    fn open_browser(&self, url: &str) {
        if let Err(e) = open::that(url) {
            warn!("Could not auto-open browser: {}", e);
            info!("Please manually open: {}", url);
        }
    }

    /// Load blob names from index file
    fn load_blob_names(&self, project_root: &Path) -> Vec<String> {
        let index_file_path = get_index_file_path(project_root);

        if !index_file_path.exists() {
            return Vec::new();
        }

        match std::fs::read_to_string(&index_file_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(names) => names,
                Err(e) => {
                    warn!("Failed to parse index file: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                warn!("Failed to read index file: {}", e);
                Vec::new()
            }
        }
    }

    /// Call prompt-enhancer API
    async fn call_prompt_enhancer_api(
        &self,
        original_prompt: &str,
        conversation_history: &str,
        blob_names: &[String],
    ) -> Result<String> {
        call_prompt_enhancer_api_static(
            &self.client,
            &self.config,
            original_prompt,
            conversation_history,
            blob_names,
        )
        .await
    }
}

/// Static function to call prompt-enhancer API (used for callback)
async fn call_prompt_enhancer_api_static(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
    blob_names: &[String],
) -> Result<String> {
    if use_new_endpoint() {
        info!("Using NEW prompt-enhancer endpoint");
        call_new_endpoint(client, config, original_prompt, conversation_history).await
    } else {
        info!("Using OLD chat-stream endpoint");
        call_old_endpoint(
            client,
            config,
            original_prompt,
            conversation_history,
            blob_names,
        )
        .await
    }
}

/// Call NEW /prompt-enhancer endpoint (simplified, matches augment.mjs)
async fn call_new_endpoint(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
) -> Result<String> {
    let chat_history = parse_chat_history(conversation_history);

    // Build simplified request payload (matches augment.mjs promptEnhancer)
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

    // Lazy serialization: only build log if logging is enabled
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
                &config.token,
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

/// Render the enhance prompt template safely without corrupting user input
/// Uses split+concat instead of replace to avoid replacing placeholders
/// that may appear in user content
/// Note: Template only has {original_prompt} placeholder (matching augment.mjs)
pub fn render_enhance_prompt(original_prompt: &str) -> Result<String> {
    let (before, after) = ENHANCE_PROMPT_TEMPLATE
        .split_once("{original_prompt}")
        .ok_or_else(|| anyhow!("ENHANCE_PROMPT_TEMPLATE missing {{original_prompt}}"))?;

    let mut rendered = String::with_capacity(before.len() + original_prompt.len() + after.len());
    rendered.push_str(before);
    rendered.push_str(original_prompt);
    rendered.push_str(after);
    Ok(rendered)
}

/// Call OLD /chat-stream endpoint (full request with blobs)
async fn call_old_endpoint(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
    blob_names: &[String],
) -> Result<String> {
    let chat_history = parse_chat_history(conversation_history);

    // Build final prompt using template (safe rendering to preserve user content)
    let final_prompt = render_enhance_prompt(original_prompt)?;

    // Detect language for user guidelines
    let is_chinese = is_chinese_text(original_prompt);
    let language_guideline = if is_chinese {
        "Please respond in Chinese (Simplified Chinese). 请用中文回复。".to_string()
    } else {
        String::new()
    };

    // Sort blob names as augment.mjs does via gS() function
    let mut sorted_blob_names = blob_names.to_vec();
    sorted_blob_names.sort();

    // Build full request payload (matches augment.mjs chatStream request structure)
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

    // Lazy serialization: only build log if logging is enabled
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
                &config.token,
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

/// Handle API response text (after body has been extracted)
/// For NEW endpoint: returns text directly
/// For OLD endpoint: first tries streaming parse (line-by-line JSON), then extracts from XML tag
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
        // OLD endpoint returns streaming response (line-by-line JSON)
        // Try streaming parse first, then fall back to single JSON parse
        parse_streaming_response(body_text)?
    } else {
        // NEW endpoint returns single JSON response
        let resp: PromptEnhancerResponse = serde_json::from_str(body_text)
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;
        resp.text
            .ok_or_else(|| anyhow!("Prompt enhancer API returned empty result"))?
    };

    // For OLD endpoint, parse XML tag to extract enhanced prompt
    let enhanced_text = if is_old_endpoint {
        extract_enhanced_prompt(&enhanced_text).unwrap_or_else(|| enhanced_text.clone())
    } else {
        enhanced_text
    };

    // Replace Augment-specific tool names with ace-tool names
    let enhanced_text = replace_tool_names(&enhanced_text);

    Ok(enhanced_text)
}

/// Parse streaming response from /chat-stream endpoint
/// Response format: each line is a JSON object with a "text" field
/// Concatenates all text fields to build the complete response
pub fn parse_streaming_response(body_text: &str) -> Result<String> {
    let mut combined_text = String::new();
    let mut parsed_any = false;

    for line in body_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to parse each line as JSON
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
        // Fall back to single JSON parse if streaming parse failed
        let resp: PromptEnhancerResponse = serde_json::from_str(body_text)
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;
        resp.text
            .ok_or_else(|| anyhow!("Prompt enhancer API returned empty result"))
    }
}

/// Extract enhanced prompt from XML-like response
/// Looks for content between <augment-enhanced-prompt> and </augment-enhanced-prompt> tags
/// Returns None if tag not found or content is empty/whitespace-only
pub fn extract_enhanced_prompt(text: &str) -> Option<String> {
    lazy_static::lazy_static! {
        // More tolerant regex: allows optional whitespace/attributes in tags
        static ref TAG_RE: Regex = Regex::new(
            r"(?s)<augment-enhanced-prompt(?:\s+[^>]*)?>\s*(.*?)\s*</augment-enhanced-prompt\s*>"
        ).unwrap();
    }

    TAG_RE.captures(text).and_then(|caps| {
        let trimmed = caps.get(1)?.as_str().trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Detect if text is primarily Chinese
/// Returns true if Chinese characters make up at least 10% of non-whitespace characters
/// or if there are at least 3 Chinese characters
pub fn is_chinese_text(text: &str) -> bool {
    lazy_static::lazy_static! {
        static ref CHINESE_RE: Regex = Regex::new(r"[\u4e00-\u9fa5]").unwrap();
    }

    let chinese_count = CHINESE_RE.find_iter(text).count();
    if chinese_count == 0 {
        return false;
    }

    // If there are at least 3 Chinese characters, consider it Chinese
    if chinese_count >= 3 {
        return true;
    }

    // Otherwise, check if Chinese makes up at least 10% of non-whitespace content
    let non_whitespace_count = text.chars().filter(|c| !c.is_whitespace()).count();
    if non_whitespace_count == 0 {
        return false;
    }

    (chinese_count as f64 / non_whitespace_count as f64) >= 0.1
}

/// Replace Augment-specific tool names with ace-tool names
pub fn replace_tool_names(text: &str) -> String {
    text.replace("codebase-retrieval", "search_context")
        .replace("codebase_retrieval", "search_context")
}

/// Parse conversation history into ChatMessage format
pub fn parse_chat_history(conversation_history: &str) -> Vec<ChatMessage> {
    let mut chat_history = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in conversation_history.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if current_role.is_some() {
                current_lines.push(String::new());
            }
            continue;
        }

        if let Some((role, content)) = parse_history_line(trimmed) {
            if let Some(prev_role) = current_role.take() {
                chat_history.push(ChatMessage {
                    role: prev_role,
                    content: current_lines.join("\n"),
                });
            }
            current_role = Some(role);
            current_lines.clear();
            current_lines.push(content);
        } else if current_role.is_some() {
            current_lines.push(line.to_string());
        }
    }

    if let Some(role) = current_role {
        chat_history.push(ChatMessage {
            role,
            content: current_lines.join("\n"),
        });
    }

    chat_history
}

fn parse_history_line(line: &str) -> Option<(String, String)> {
    let user_prefixes = ["User:", "用户:"];
    for prefix in user_prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(("user".to_string(), rest.trim().to_string()));
        }
    }

    let assistant_prefixes = ["AI:", "Assistant:", "助手:"];
    for prefix in assistant_prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(("assistant".to_string(), rest.trim().to_string()));
        }
    }

    None
}

/// Lazy static macro for regex
mod lazy_static {
    #[macro_export]
    macro_rules! lazy_static {
        ($(static ref $name:ident: $t:ty = $init:expr;)*) => {
            $(
                static $name: std::sync::LazyLock<$t> = std::sync::LazyLock::new(|| $init);
            )*
        };
    }
    pub use lazy_static;
}
