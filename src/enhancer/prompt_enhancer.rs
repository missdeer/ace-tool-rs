//! Prompt Enhancer - Core enhancement logic
//! Based on Augment VSCode plugin implementation
//!
//! Supports two API endpoints controlled by environment variable `ACE_ENHANCER_ENDPOINT`:
//! - `new`: Uses `/prompt-enhancer` endpoint (simplified request, matches augment.mjs)
//! - `old` (default): Uses `/chat-stream` endpoint (full request with blobs)

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::utils::project_detector::get_index_file_path;

use super::server::EnhancerServer;

/// Default model for prompt enhancement API
const DEFAULT_MODEL: &str = "claude-sonnet-4-5";

/// Environment variable to control which endpoint to use
/// Values: "new" (prompt-enhancer endpoint) or "old" (chat-stream endpoint, default)
const ENV_ENHANCER_ENDPOINT: &str = "ACE_ENHANCER_ENDPOINT";

/// Node ID for NEW endpoint (matches augment.mjs promptEnhancer)
const NODE_ID_NEW: i32 = 0;

/// Node ID for OLD endpoint
const NODE_ID_OLD: i32 = 1;

/// Check if we should use the new prompt-enhancer endpoint
fn use_new_endpoint() -> bool {
    std::env::var(ENV_ENHANCER_ENDPOINT)
        .map(|v| v.trim().eq_ignore_ascii_case("new"))
        .unwrap_or(false)
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
#[derive(Debug, Serialize)]
struct PromptEnhancerRequestOld {
    nodes: Vec<PromptNode>,
    chat_history: Vec<ChatMessage>,
    blobs: BlobsPayload,
    conversation_id: Option<String>,
    model: String,
    mode: String,
    user_guided_blobs: Vec<String>,
    external_source_ids: Vec<String>,
    user_guidelines: String,
    workspace_guidelines: String,
    rules: Vec<String>,
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
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct BlobsPayload {
    checkpoint_id: Option<String>,
    added_blobs: Vec<String>,
    deleted_blobs: Vec<String>,
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

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    handle_response(response).await
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

    // Detect language for user guidelines
    let is_chinese = is_chinese_text(original_prompt);
    let language_guideline = if is_chinese {
        "Please respond in Chinese (Simplified Chinese). 请用中文回复。".to_string()
    } else {
        String::new()
    };

    // Build full request payload
    let payload = PromptEnhancerRequestOld {
        nodes: vec![PromptNode {
            id: NODE_ID_OLD,
            node_type: 0,
            text_node: TextNode {
                content: original_prompt.to_string(),
            },
        }],
        chat_history,
        blobs: BlobsPayload {
            checkpoint_id: None,
            added_blobs: blob_names.to_vec(),
            deleted_blobs: Vec::new(),
        },
        conversation_id: None,
        model: DEFAULT_MODEL.to_string(),
        mode: "CHAT".to_string(),
        user_guided_blobs: Vec::new(),
        external_source_ids: Vec::new(),
        user_guidelines: language_guideline,
        workspace_guidelines: String::new(),
        rules: Vec::new(),
    };

    let url = format!("{}/chat-stream", config.base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    handle_response(response).await
}

/// Handle API response
async fn handle_response(response: reqwest::Response) -> Result<String> {
    let status = response.status();

    if status == 401 {
        return Err(anyhow!("Token invalid or expired"));
    }
    if status == 403 {
        return Err(anyhow!("Access denied, token may be disabled"));
    }
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Prompt enhancer API failed: {} - {}", status, text));
    }

    let resp: PromptEnhancerResponse = response.json().await?;

    let enhanced_text = resp
        .text
        .ok_or_else(|| anyhow!("Prompt enhancer API returned empty result"))?;

    // Replace Augment-specific tool names with ace-tool names
    let enhanced_text = replace_tool_names(&enhanced_text);

    Ok(enhanced_text)
}

/// Detect if text is primarily Chinese
/// Returns true if Chinese characters make up at least 10% of non-whitespace characters
/// or if there are at least 3 Chinese characters
fn is_chinese_text(text: &str) -> bool {
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
fn replace_tool_names(text: &str) -> String {
    text.replace("codebase-retrieval", "search_context")
        .replace("codebase_retrieval", "search_context")
}

/// Parse conversation history into ChatMessage format
fn parse_chat_history(conversation_history: &str) -> Vec<ChatMessage> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // is_chinese_text Tests
    // ========================================================================

    #[test]
    fn test_is_chinese_text() {
        assert!(is_chinese_text("你好世界")); // 4 Chinese chars >= 3
        assert!(is_chinese_text("Hello 中文好")); // 3 Chinese chars >= 3
        assert!(!is_chinese_text("Hello World"));
        assert!(!is_chinese_text("123"));
    }

    #[test]
    fn test_is_chinese_text_pure_chinese() {
        assert!(is_chinese_text("这是纯中文文本")); // Many Chinese chars
        assert!(is_chinese_text("中")); // 1 Chinese char = 100% of content
    }

    #[test]
    fn test_is_chinese_text_mixed() {
        assert!(is_chinese_text("Hello中文World")); // 2 Chinese chars but > 10% of non-ws
        assert!(is_chinese_text("123中456")); // 1 Chinese char but > 10% (1/6 = 16%)
        assert!(is_chinese_text("test 测试 test")); // 2 Chinese chars, 2/12 = 16%
    }

    #[test]
    fn test_is_chinese_text_threshold() {
        // Test the 10% threshold
        assert!(!is_chinese_text("This is a very long English text with 中")); // 1 char, < 10%
        assert!(is_chinese_text("中文测试")); // 4 chars >= 3
        assert!(is_chinese_text("abc中文")); // 2 Chinese chars, 2/5 = 40%
    }

    #[test]
    fn test_is_chinese_text_empty() {
        assert!(!is_chinese_text(""));
    }

    #[test]
    fn test_is_chinese_text_whitespace_only() {
        assert!(!is_chinese_text("   "));
        assert!(!is_chinese_text("\t\n"));
    }

    #[test]
    fn test_is_chinese_text_special_chars() {
        assert!(!is_chinese_text("@#$%^&*()"));
        assert!(!is_chinese_text(".,;:!?"));
    }

    #[test]
    fn test_is_chinese_text_japanese() {
        // Japanese hiragana/katakana should not match Chinese regex
        assert!(!is_chinese_text("こんにちは")); // Hiragana
        assert!(!is_chinese_text("カタカナ")); // Katakana
    }

    #[test]
    fn test_is_chinese_text_korean() {
        assert!(!is_chinese_text("안녕하세요")); // Korean
    }

    #[test]
    fn test_is_chinese_text_numbers_and_punctuation() {
        assert!(!is_chinese_text("12345"));
        assert!(!is_chinese_text("..."));
        assert!(is_chinese_text("数字123")); // 2 Chinese chars, 2/5 = 40%
    }

    #[test]
    fn test_is_chinese_text_chinese_punctuation() {
        // Chinese punctuation alone doesn't make it Chinese
        assert!(!is_chinese_text("。，！？"));
        // But with Chinese characters, it should
        assert!(is_chinese_text("你好！")); // 2 Chinese chars, 2/3 = 66%
    }

    // ========================================================================
    // replace_tool_names Tests
    // ========================================================================

    #[test]
    fn test_replace_tool_names() {
        let text = "Use codebase-retrieval to search";
        let result = replace_tool_names(text);
        assert_eq!(result, "Use search_context to search");

        let text2 = "Use codebase_retrieval to search";
        let result2 = replace_tool_names(text2);
        assert_eq!(result2, "Use search_context to search");
    }

    #[test]
    fn test_replace_tool_names_multiple_occurrences() {
        let text = "First codebase-retrieval then codebase-retrieval again";
        let result = replace_tool_names(text);
        assert_eq!(result, "First search_context then search_context again");
    }

    #[test]
    fn test_replace_tool_names_mixed() {
        let text = "Use codebase-retrieval and codebase_retrieval";
        let result = replace_tool_names(text);
        assert_eq!(result, "Use search_context and search_context");
    }

    #[test]
    fn test_replace_tool_names_no_match() {
        let text = "Use search_context directly";
        let result = replace_tool_names(text);
        assert_eq!(result, "Use search_context directly");
    }

    #[test]
    fn test_replace_tool_names_empty() {
        let result = replace_tool_names("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_replace_tool_names_preserves_case() {
        let text = "CODEBASE-RETRIEVAL"; // Won't match (case sensitive)
        let result = replace_tool_names(text);
        assert_eq!(result, "CODEBASE-RETRIEVAL");
    }

    #[test]
    fn test_replace_tool_names_in_code_block() {
        let text = "```\ncodebase-retrieval query\n```";
        let result = replace_tool_names(text);
        assert!(result.contains("search_context"));
    }

    #[test]
    fn test_replace_tool_names_in_json() {
        let text = r#"{"tool": "codebase-retrieval", "args": {}}"#;
        let result = replace_tool_names(text);
        assert!(result.contains("search_context"));
    }

    // ========================================================================
    // parse_chat_history Tests
    // ========================================================================

    #[test]
    fn test_parse_chat_history() {
        let history = "User: Hello\nAssistant: Hi there\n用户: 你好\n助手: 你好！";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Hello");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[1].content, "Hi there");
        assert_eq!(result[2].role, "user");
        assert_eq!(result[2].content, "你好");
        assert_eq!(result[3].role, "assistant");
        assert_eq!(result[3].content, "你好！");
    }

    #[test]
    fn test_parse_chat_history_empty() {
        let result = parse_chat_history("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_chat_history_whitespace_only() {
        let result = parse_chat_history("   \n\t\n   ");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_chat_history_user_only() {
        let history = "User: Hello world";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Hello world");
    }

    #[test]
    fn test_parse_chat_history_assistant_only() {
        let history = "Assistant: I can help with that";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        assert_eq!(result[0].content, "I can help with that");
    }

    #[test]
    fn test_parse_chat_history_ai_prefix() {
        let history = "AI: This is an AI response";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        assert_eq!(result[0].content, "This is an AI response");
    }

    #[test]
    fn test_parse_chat_history_chinese_prefixes() {
        let history = "用户: 你好\n助手: 你好！有什么可以帮助你的？";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "你好");
        assert_eq!(result[1].role, "assistant");
        assert!(result[1].content.contains("帮助"));
    }

    #[test]
    fn test_parse_chat_history_with_extra_whitespace() {
        let history = "User:   Hello with spaces   ";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "Hello with spaces");
    }

    #[test]
    fn test_parse_chat_history_ignores_unknown_prefixes() {
        let history = "System: Internal message\nUser: Hello";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Hello");
    }

    #[test]
    fn test_parse_chat_history_with_colons_in_content() {
        let history = "User: Time is 10:30:00";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "Time is 10:30:00");
    }

    #[test]
    fn test_parse_chat_history_multiline_message() {
        let history = "User: Line 1\nLine 2\nAssistant: Response";
        let result = parse_chat_history(history);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Line 1\nLine 2");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[1].content, "Response");
    }

    #[test]
    fn test_parse_chat_history_long_conversation() {
        let history = (0..20)
            .map(|i| format!("User: Message {}\nAssistant: Response {}", i, i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = parse_chat_history(&history);
        assert_eq!(result.len(), 40);
    }

    // ========================================================================
    // ChatMessage Tests
    // ========================================================================

    #[test]
    fn test_chat_message_clone() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };

        let cloned = msg.clone();
        assert_eq!(cloned.role, msg.role);
        assert_eq!(cloned.content, msg.content);
    }

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_chat_message_deserialization() {
        let json = r#"{"role":"assistant","content":"Hi there"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();

        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hi there");
    }

    // ========================================================================
    // Integration-like Tests
    // ========================================================================

    #[test]
    fn test_language_detection_for_enhancement() {
        // Simulate language detection for API guideline
        let chinese_prompt = "添加一个登录功能";
        let english_prompt = "Add a login feature";

        assert!(is_chinese_text(chinese_prompt));
        assert!(!is_chinese_text(english_prompt));
    }

    #[test]
    fn test_full_workflow_simulation() {
        // Simulate a typical enhancement workflow
        let original_prompt = "新加一个登录页面";
        let conversation = "User: 我在开发一个web应用\n助手: 好的，我可以帮助你";

        // Check language detection
        assert!(is_chinese_text(original_prompt));

        // Parse conversation
        let history = parse_chat_history(conversation);
        assert_eq!(history.len(), 2);

        // Simulate enhanced output with tool name replacement
        let enhanced = "请使用 codebase-retrieval 工具来搜索";
        let replaced = replace_tool_names(enhanced);
        assert!(replaced.contains("search_context"));
    }

    // ========================================================================
    // Endpoint Selection Tests
    // Note: Environment variable tests are combined into one to avoid race conditions
    // ========================================================================

    #[test]
    fn test_use_new_endpoint_all_cases() {
        // Use a static mutex to prevent parallel execution issues
        use std::sync::Mutex;
        static ENV_MUTEX: Mutex<()> = Mutex::new(());
        let _guard = ENV_MUTEX.lock().unwrap();

        // Save original value to restore later
        let original_value = std::env::var(ENV_ENHANCER_ENDPOINT).ok();

        // Test 1: Default should be old endpoint
        std::env::remove_var(ENV_ENHANCER_ENDPOINT);
        assert!(!use_new_endpoint(), "Default should use old endpoint");

        // Test 2: Explicit "new" should use new endpoint
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "new");
        assert!(use_new_endpoint(), "\"new\" should use new endpoint");

        // Test 3: Case insensitive - "NEW"
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "NEW");
        assert!(use_new_endpoint(), "\"NEW\" should use new endpoint");

        // Test 4: Case insensitive - "New"
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "New");
        assert!(use_new_endpoint(), "\"New\" should use new endpoint");

        // Test 5: Explicit "old" should use old endpoint
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "old");
        assert!(!use_new_endpoint(), "\"old\" should use old endpoint");

        // Test 6: Invalid value should default to old endpoint
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "invalid");
        assert!(!use_new_endpoint(), "Invalid value should use old endpoint");

        // Test 7: Whitespace should be trimmed
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "  new  ");
        assert!(use_new_endpoint(), "Whitespace should be trimmed");

        // Test 8: Empty string should use old endpoint
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "");
        assert!(!use_new_endpoint(), "Empty string should use old endpoint");

        // Test 9: Whitespace only should use old endpoint
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "   ");
        assert!(
            !use_new_endpoint(),
            "Whitespace only should use old endpoint"
        );

        // Test 10: Newlines should be trimmed
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "\nnew\n");
        assert!(use_new_endpoint(), "Newlines should be trimmed");

        // Test 11: Mixed case variations
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "nEw");
        assert!(use_new_endpoint(), "Mixed case nEw should work");

        std::env::set_var(ENV_ENHANCER_ENDPOINT, "nEW");
        assert!(use_new_endpoint(), "Mixed case nEW should work");

        // Test 12: Similar but different values
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "newer");
        assert!(!use_new_endpoint(), "\"newer\" is not \"new\"");

        std::env::set_var(ENV_ENHANCER_ENDPOINT, "new1");
        assert!(!use_new_endpoint(), "\"new1\" is not \"new\"");

        std::env::set_var(ENV_ENHANCER_ENDPOINT, "anew");
        assert!(!use_new_endpoint(), "\"anew\" is not \"new\"");

        // Test 13: Tabs in value
        std::env::set_var(ENV_ENHANCER_ENDPOINT, "\tnew\t");
        assert!(use_new_endpoint(), "Tabs should be trimmed");

        // Test 14: Mixed whitespace
        std::env::set_var(ENV_ENHANCER_ENDPOINT, " \t\nnew\n\t ");
        assert!(use_new_endpoint(), "Mixed whitespace should be trimmed");

        // Restore original value
        match original_value {
            Some(v) => std::env::set_var(ENV_ENHANCER_ENDPOINT, v),
            None => std::env::remove_var(ENV_ENHANCER_ENDPOINT),
        }
    }

    // ========================================================================
    // Request Struct Serialization Tests
    // ========================================================================

    #[test]
    fn test_new_request_serialization() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "test prompt".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: "test-model".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":0"));
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"mode\":\"CHAT\""));
        // Should NOT contain blobs, user_guidelines, etc.
        assert!(!json.contains("blobs"));
        assert!(!json.contains("user_guidelines"));
    }

    #[test]
    fn test_old_request_serialization() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: "test prompt".to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec!["blob1".to_string()],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test-model".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "test guideline".to_string(),
            workspace_guidelines: String::new(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("blobs"));
        assert!(json.contains("user_guidelines"));
        assert!(json.contains("blob1"));
    }

    #[test]
    fn test_new_request_node_id_is_zero() {
        // Verify new endpoint uses id: 0 like augment.mjs
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        assert_eq!(request.nodes[0].id, NODE_ID_NEW);
    }

    // ========================================================================
    // NODE_ID Constants Tests
    // ========================================================================

    #[test]
    fn test_node_id_constants_values() {
        assert_eq!(NODE_ID_NEW, 0);
        assert_eq!(NODE_ID_OLD, 1);
    }

    #[test]
    fn test_node_id_constants_are_different() {
        assert_ne!(NODE_ID_NEW, NODE_ID_OLD);
    }

    // ========================================================================
    // PromptEnhancerRequestNew Tests
    // ========================================================================

    #[test]
    fn test_new_request_with_chat_history() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![
                ChatMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: "Hi there".to_string(),
                },
            ],
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"role\":\"assistant\""));
        assert!(json.contains("\"content\":\"Hello\""));
        assert!(json.contains("\"content\":\"Hi there\""));
    }

    #[test]
    fn test_new_request_conversation_id_none_skipped() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("conversation_id"));
    }

    #[test]
    fn test_new_request_conversation_id_some_included() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: Some("conv-123".to_string()),
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"conversation_id\":\"conv-123\""));
    }

    #[test]
    fn test_new_request_default_model() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-sonnet-4-5"));
    }

    #[test]
    fn test_new_request_mode_chat() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"mode\":\"CHAT\""));
    }

    #[test]
    fn test_new_request_multiple_nodes() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![
                PromptNode {
                    id: NODE_ID_NEW,
                    node_type: 0,
                    text_node: TextNode {
                        content: "first".to_string(),
                    },
                },
                PromptNode {
                    id: NODE_ID_NEW + 1,
                    node_type: 1,
                    text_node: TextNode {
                        content: "second".to_string(),
                    },
                },
            ],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"content\":\"first\""));
        assert!(json.contains("\"content\":\"second\""));
    }

    #[test]
    fn test_new_request_empty_prompt() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"content\":\"\""));
    }

    #[test]
    fn test_new_request_unicode_content() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "你好世界 🌍".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("你好世界"));
    }

    #[test]
    fn test_new_request_special_characters() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "line1\nline2\ttab\"quote".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\\n"));
        assert!(json.contains("\\t"));
        assert!(json.contains("\\\""));
    }

    // ========================================================================
    // PromptEnhancerRequestOld Tests
    // ========================================================================

    #[test]
    fn test_old_request_with_blobs() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: Some("cp-123".to_string()),
                added_blobs: vec!["blob1".to_string(), "blob2".to_string()],
                deleted_blobs: vec!["blob3".to_string()],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"checkpoint_id\":\"cp-123\""));
        assert!(json.contains("\"blob1\""));
        assert!(json.contains("\"blob2\""));
        assert!(json.contains("\"blob3\""));
        assert!(json.contains("\"added_blobs\""));
        assert!(json.contains("\"deleted_blobs\""));
    }

    #[test]
    fn test_old_request_empty_blobs() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"added_blobs\":[]"));
        assert!(json.contains("\"deleted_blobs\":[]"));
    }

    #[test]
    fn test_old_request_with_guidelines() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "User guideline text".to_string(),
            workspace_guidelines: "Workspace guideline text".to_string(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"user_guidelines\":\"User guideline text\""));
        assert!(json.contains("\"workspace_guidelines\":\"Workspace guideline text\""));
    }

    #[test]
    fn test_old_request_with_rules() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec!["rule1".to_string(), "rule2".to_string()],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"rules\":[\"rule1\",\"rule2\"]"));
    }

    #[test]
    fn test_old_request_with_external_sources() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec!["guided1".to_string()],
            external_source_ids: vec!["ext1".to_string(), "ext2".to_string()],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"user_guided_blobs\":[\"guided1\"]"));
        assert!(json.contains("\"external_source_ids\":[\"ext1\",\"ext2\"]"));
    }

    #[test]
    fn test_old_request_node_id_is_one() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        assert_eq!(request.nodes[0].id, NODE_ID_OLD);
        assert_eq!(request.nodes[0].id, 1);
    }

    #[test]
    fn test_old_request_chinese_guidelines() {
        let request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "Please respond in Chinese (Simplified Chinese). 请用中文回复。"
                .to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("请用中文回复"));
    }

    // ========================================================================
    // BlobsPayload Tests
    // ========================================================================

    #[test]
    fn test_blobs_payload_serialization() {
        let blobs = BlobsPayload {
            checkpoint_id: Some("cp-abc".to_string()),
            added_blobs: vec!["a".to_string(), "b".to_string()],
            deleted_blobs: vec!["c".to_string()],
        };

        let json = serde_json::to_string(&blobs).unwrap();
        assert!(json.contains("\"checkpoint_id\":\"cp-abc\""));
        assert!(json.contains("\"added_blobs\":[\"a\",\"b\"]"));
        assert!(json.contains("\"deleted_blobs\":[\"c\"]"));
    }

    #[test]
    fn test_blobs_payload_checkpoint_id_null() {
        let blobs = BlobsPayload {
            checkpoint_id: None,
            added_blobs: vec![],
            deleted_blobs: vec![],
        };

        let json = serde_json::to_string(&blobs).unwrap();
        assert!(json.contains("\"checkpoint_id\":null"));
    }

    #[test]
    fn test_blobs_payload_many_blobs() {
        let blobs = BlobsPayload {
            checkpoint_id: None,
            added_blobs: (0..100).map(|i| format!("blob_{}", i)).collect(),
            deleted_blobs: vec![],
        };

        let json = serde_json::to_string(&blobs).unwrap();
        assert!(json.contains("blob_0"));
        assert!(json.contains("blob_99"));
    }

    // ========================================================================
    // PromptNode and TextNode Tests
    // ========================================================================

    #[test]
    fn test_prompt_node_serialization() {
        let node = PromptNode {
            id: 5,
            node_type: 2,
            text_node: TextNode {
                content: "test content".to_string(),
            },
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"id\":5"));
        assert!(json.contains("\"type\":2")); // Renamed via serde
        assert!(json.contains("\"text_node\""));
        assert!(json.contains("\"content\":\"test content\""));
    }

    #[test]
    fn test_prompt_node_type_renamed() {
        let node = PromptNode {
            id: 0,
            node_type: 0,
            text_node: TextNode {
                content: "".to_string(),
            },
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"type\":"));
        assert!(!json.contains("\"node_type\":"));
    }

    #[test]
    fn test_text_node_serialization() {
        let text_node = TextNode {
            content: "Hello World".to_string(),
        };

        let json = serde_json::to_string(&text_node).unwrap();
        assert_eq!(json, r#"{"content":"Hello World"}"#);
    }

    #[test]
    fn test_text_node_empty() {
        let text_node = TextNode {
            content: "".to_string(),
        };

        let json = serde_json::to_string(&text_node).unwrap();
        assert_eq!(json, r#"{"content":""}"#);
    }

    #[test]
    fn test_text_node_long_content() {
        let long_content = "a".repeat(10000);
        let text_node = TextNode {
            content: long_content.clone(),
        };

        let json = serde_json::to_string(&text_node).unwrap();
        assert!(json.contains(&long_content));
    }

    // ========================================================================
    // PromptEnhancerResponse Tests
    // ========================================================================

    #[test]
    fn test_response_deserialization_with_text() {
        let json = r#"{"text":"Enhanced prompt text"}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.text, Some("Enhanced prompt text".to_string()));
    }

    #[test]
    fn test_response_deserialization_null_text() {
        let json = r#"{"text":null}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert!(response.text.is_none());
    }

    #[test]
    fn test_response_deserialization_missing_text() {
        let json = r#"{}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert!(response.text.is_none());
    }

    #[test]
    fn test_response_deserialization_extra_fields_ignored() {
        let json = r#"{"text":"test","extra_field":"ignored","another":123}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.text, Some("test".to_string()));
    }

    #[test]
    fn test_response_deserialization_unicode() {
        let json = r#"{"text":"增强后的提示 🚀"}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.text, Some("增强后的提示 🚀".to_string()));
    }

    #[test]
    fn test_response_deserialization_multiline() {
        let json = r#"{"text":"line1\nline2\nline3"}"#;
        let response: PromptEnhancerResponse = serde_json::from_str(json).unwrap();

        assert!(response.text.as_ref().unwrap().contains('\n'));
    }

    // ========================================================================
    // Request Structure Comparison Tests
    // ========================================================================

    #[test]
    fn test_new_vs_old_request_field_difference() {
        let new_request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
        };

        let old_request = PromptEnhancerRequestOld {
            nodes: vec![],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: "test".to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let new_json = serde_json::to_string(&new_request).unwrap();
        let old_json = serde_json::to_string(&old_request).unwrap();

        // New request should NOT have these fields
        assert!(!new_json.contains("blobs"));
        assert!(!new_json.contains("user_guided_blobs"));
        assert!(!new_json.contains("external_source_ids"));
        assert!(!new_json.contains("user_guidelines"));
        assert!(!new_json.contains("workspace_guidelines"));
        assert!(!new_json.contains("rules"));

        // Old request SHOULD have these fields
        assert!(old_json.contains("blobs"));
        assert!(old_json.contains("user_guided_blobs"));
        assert!(old_json.contains("external_source_ids"));
        assert!(old_json.contains("user_guidelines"));
        assert!(old_json.contains("workspace_guidelines"));
        assert!(old_json.contains("rules"));
    }

    #[test]
    fn test_new_request_is_smaller() {
        let new_request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![],
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        let old_request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: "test".to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: "".to_string(),
            workspace_guidelines: "".to_string(),
            rules: vec![],
        };

        let new_json = serde_json::to_string(&new_request).unwrap();
        let old_json = serde_json::to_string(&old_request).unwrap();

        assert!(new_json.len() < old_json.len());
    }

    // ========================================================================
    // Default Model Tests
    // ========================================================================

    #[test]
    fn test_default_model_constant() {
        assert_eq!(DEFAULT_MODEL, "claude-sonnet-4-5");
    }

    #[test]
    fn test_default_model_in_request() {
        let request = PromptEnhancerRequestNew {
            nodes: vec![],
            chat_history: vec![],
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        assert_eq!(request.model, "claude-sonnet-4-5");
    }

    // ========================================================================
    // Environment Variable Edge Cases Tests
    // ========================================================================

    #[test]
    fn test_env_enhancer_endpoint_constant() {
        assert_eq!(ENV_ENHANCER_ENDPOINT, "ACE_ENHANCER_ENDPOINT");
    }

    // ========================================================================
    // Integration Tests for Request Building
    // ========================================================================

    #[test]
    fn test_build_new_request_from_parsed_history() {
        let history = "User: Hello\nAssistant: Hi";
        let chat_history = parse_chat_history(history);

        let request = PromptEnhancerRequestNew {
            nodes: vec![PromptNode {
                id: NODE_ID_NEW,
                node_type: 0,
                text_node: TextNode {
                    content: "test prompt".to_string(),
                },
            }],
            chat_history,
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
        };

        assert_eq!(request.chat_history.len(), 2);
        assert_eq!(request.chat_history[0].role, "user");
        assert_eq!(request.chat_history[1].role, "assistant");
    }

    #[test]
    fn test_build_old_request_with_language_detection() {
        let chinese_prompt = "添加登录功能";
        let is_chinese = is_chinese_text(chinese_prompt);

        let language_guideline = if is_chinese {
            "Please respond in Chinese (Simplified Chinese). 请用中文回复。".to_string()
        } else {
            String::new()
        };

        let request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: chinese_prompt.to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: language_guideline.clone(),
            workspace_guidelines: String::new(),
            rules: vec![],
        };

        assert!(request.user_guidelines.contains("请用中文回复"));
    }

    #[test]
    fn test_build_old_request_english_no_guideline() {
        let english_prompt = "Add a login feature";
        let is_chinese = is_chinese_text(english_prompt);

        let language_guideline = if is_chinese {
            "Please respond in Chinese (Simplified Chinese). 请用中文回复。".to_string()
        } else {
            String::new()
        };

        let request = PromptEnhancerRequestOld {
            nodes: vec![PromptNode {
                id: NODE_ID_OLD,
                node_type: 0,
                text_node: TextNode {
                    content: english_prompt.to_string(),
                },
            }],
            chat_history: vec![],
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: vec![],
                deleted_blobs: vec![],
            },
            conversation_id: None,
            model: DEFAULT_MODEL.to_string(),
            mode: "CHAT".to_string(),
            user_guided_blobs: vec![],
            external_source_ids: vec![],
            user_guidelines: language_guideline.clone(),
            workspace_guidelines: String::new(),
            rules: vec![],
        };

        assert!(request.user_guidelines.is_empty());
    }
}
