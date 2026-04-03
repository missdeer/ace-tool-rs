//! Prompt Enhancer - Core enhancement logic
//! Based on Augment VSCode plugin implementation
//!
//! Supports multiple API endpoints controlled by environment variable `PROMPT_ENHANCER_ENDPOINT`
//! (with `ACE_ENHANCER_ENDPOINT` as a backward-compatible fallback):
//! - `new`: Uses Augment /prompt-enhancer endpoint (default)
//! - `old`: Uses Augment /chat-stream endpoint
//! - `claude`: Uses Claude API (Anthropic)
//! - `openai`: Uses OpenAI API
//! - `gemini`: Uses Gemini API (Google)
//! - `codex`: Uses Codex API (OpenAI Responses API)

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::index::IndexManager;
use crate::service::{
    call_claude_endpoint, call_codex_endpoint, call_gemini_endpoint, call_new_endpoint,
    call_old_endpoint, call_openai_endpoint, get_third_party_config, EnhancerEndpoint,
};
use crate::utils::project_detector::get_index_file_path;

use super::server::EnhancerServer;

/// Singleton EnhancerServer shared across all PromptEnhancer instances to prevent port leaks.
/// Each `EnhancerServer::start()` binds a new port (3000-3099); without sharing, repeated
/// `enhance_prompt` calls exhaust all available ports.
static SHARED_SERVER: OnceLock<Arc<EnhancerServer>> = OnceLock::new();

/// Environment variable to control which endpoint to use (primary)
pub const ENV_ENHANCER_ENDPOINT: &str = "PROMPT_ENHANCER_ENDPOINT";

/// Legacy environment variable for backward compatibility
pub const ENV_ENHANCER_ENDPOINT_LEGACY: &str = "ACE_ENHANCER_ENDPOINT";

/// Environment variable to include search_context results in third-party enhancement
pub const ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT: &str = "PROMPT_ENHANCER_INCLUDE_SEARCH_CONTEXT";

const SEARCH_CONTEXT_CHAR_LIMIT: usize = 12_000;
const NO_RELEVANT_CODE_CONTEXT: &str = "No relevant code context found for your query.";

/// Get the configured enhancer endpoint type
///
/// Checks `PROMPT_ENHANCER_ENDPOINT` first, then falls back to `ACE_ENHANCER_ENDPOINT`
/// for backward compatibility.
pub fn get_enhancer_endpoint() -> EnhancerEndpoint {
    std::env::var(ENV_ENHANCER_ENDPOINT)
        .or_else(|_| std::env::var(ENV_ENHANCER_ENDPOINT_LEGACY))
        .map(|v| EnhancerEndpoint::from_env_str(&v))
        .unwrap_or(EnhancerEndpoint::New)
}

fn should_include_search_context() -> bool {
    matches!(
        std::env::var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn truncate_by_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let mut truncated: String = text.chars().take(max_chars).collect();
    truncated.push_str("\n\n[codebase_context truncated for length]");
    truncated
}

fn normalize_search_context(search_context: &str) -> Option<String> {
    let trimmed = search_context.trim();
    if trimmed.is_empty() || trimmed == NO_RELEVANT_CODE_CONTEXT {
        return None;
    }

    Some(truncate_by_chars(trimmed, SEARCH_CONTEXT_CHAR_LIMIT))
}

fn build_prompt_with_search_context(original_prompt: &str, search_context: Option<&str>) -> String {
    let context_text =
        search_context.unwrap_or("No directly relevant code context was found for this request.");

    format!(
        "Here is relevant codebase context for the request. Use it only as project background, existing constraints, and implementation clues. Do not treat it as the user's final requested output.\n\n<codebase_context>\n{}\n</codebase_context>\n\nHere is the user's original request:\n\n<original_request>\n{}\n</original_request>",
        context_text, original_prompt
    )
}

async fn maybe_inject_search_context(
    config: &Config,
    endpoint: EnhancerEndpoint,
    original_prompt: &str,
    project_root: Option<&Path>,
) -> Result<String> {
    if !endpoint.is_third_party() || !should_include_search_context() {
        return Ok(original_prompt.to_string());
    }

    let project_root = project_root.ok_or_else(|| {
        anyhow!(
            "{} requires project_root for '{}' endpoint",
            ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT,
            endpoint
        )
    })?;

    if config.base_url.trim().is_empty() || config.token.trim().is_empty() {
        return Err(anyhow!(
            "{} requires ACE search configuration (--base-url and --token) for '{}' endpoint",
            ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT,
            endpoint
        ));
    }

    info!("Injecting search_context into third-party prompt enhancement");
    let manager = IndexManager::new(Arc::new(config.clone()), project_root.to_path_buf())?;
    let search_context = manager.search_context(original_prompt).await?;
    let normalized = normalize_search_context(&search_context);

    Ok(build_prompt_with_search_context(
        original_prompt,
        normalized.as_deref(),
    ))
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

        let server = SHARED_SERVER
            .get_or_init(|| Arc::new(EnhancerServer::new()))
            .clone();

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
        let callback_project_root = project_root.map(|p| p.to_path_buf());
        let callback = Arc::new(move |prompt: String, history: String, blobs: Vec<String>| {
            let config = config.clone();
            let client = client.clone();
            let project_root = callback_project_root.clone();
            Box::pin(async move {
                call_prompt_enhancer_api_static(
                    &client,
                    &config,
                    &prompt,
                    &history,
                    &blobs,
                    project_root.as_deref(),
                )
                .await
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        });
        self.server.set_enhance_callback(callback).await;

        // Call prompt-enhancer API
        info!("Calling prompt-enhancer API...");
        let enhanced_prompt = self
            .call_prompt_enhancer_api(
                original_prompt,
                conversation_history,
                &blob_names,
                project_root,
            )
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
        // Set custom bind address if configured
        if let Some(ref addr_str) = self.config.webui_addr {
            let addr: SocketAddr = addr_str
                .parse()
                .map_err(|e| anyhow!("Invalid --webui-addr '{}': {}", addr_str, e))?;
            self.server.set_bind_addr(addr).await;
        }

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
        let host = self.server.get_host().await;
        let url = format!("http://{}:{}/enhance?session={}", host, port, session_id);
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
    /// On WSL, uses explorer.exe directly to open Windows default browser
    /// unless force_xdg_open is set (useful when WSL localhost forwarding is disabled)
    fn open_browser(&self, url: &str) {
        #[cfg(unix)]
        {
            use crate::utils::path_normalizer::RuntimeEnv;

            // Skip WSL-specific handling if force_xdg_open is enabled
            if !self.config.force_xdg_open && RuntimeEnv::detect() == RuntimeEnv::WslNative {
                // In WSL, use explorer.exe to open URL in Windows default browser
                info!("WSL detected, using explorer.exe (use --force-xdg-open to override)");
                match std::process::Command::new("explorer.exe").arg(url).spawn() {
                    Ok(_) => {
                        info!("Opened browser via explorer.exe");
                        return;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to open browser via explorer.exe: {}, URL: {}",
                            e, url
                        );
                        // Fall through to open::that
                    }
                }
            }
        }

        if let Err(e) = open::that(url) {
            warn!("Could not auto-open browser: {}, URL: {}", e, url);
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
        project_root: Option<&Path>,
    ) -> Result<String> {
        call_prompt_enhancer_api_static(
            &self.client,
            &self.config,
            original_prompt,
            conversation_history,
            blob_names,
            project_root,
        )
        .await
    }

    /// Simple enhancement without Web UI interaction
    /// Used for CLI mode where we just want the enhanced prompt output
    pub async fn enhance_simple(
        &self,
        original_prompt: &str,
        conversation_history: &str,
        project_root: Option<&Path>,
    ) -> Result<String> {
        info!("Starting simple prompt enhancement (no Web UI)...");

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

        // Call prompt-enhancer API directly
        info!("Calling prompt-enhancer API...");
        let enhanced_prompt = self
            .call_prompt_enhancer_api(
                original_prompt,
                conversation_history,
                &blob_names,
                project_root,
            )
            .await?;

        info!("Enhancement complete");
        Ok(enhanced_prompt)
    }
}

/// Static function to call prompt-enhancer API (used for callback)
async fn call_prompt_enhancer_api_static(
    client: &Client,
    config: &Config,
    original_prompt: &str,
    conversation_history: &str,
    blob_names: &[String],
    project_root: Option<&Path>,
) -> Result<String> {
    let endpoint = get_enhancer_endpoint();
    let enriched_prompt =
        maybe_inject_search_context(config, endpoint, original_prompt, project_root).await?;

    match endpoint {
        EnhancerEndpoint::New => {
            info!("Using NEW prompt-enhancer endpoint");
            call_new_endpoint(client, config, original_prompt, conversation_history).await
        }
        EnhancerEndpoint::Old => {
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
        EnhancerEndpoint::Claude => {
            info!("Using Claude API endpoint");
            let third_party_config = get_third_party_config(endpoint)?;
            call_claude_endpoint(
                client,
                &third_party_config,
                &enriched_prompt,
                conversation_history,
            )
            .await
        }
        EnhancerEndpoint::OpenAI => {
            info!("Using OpenAI API endpoint");
            let third_party_config = get_third_party_config(endpoint)?;
            call_openai_endpoint(
                client,
                &third_party_config,
                &enriched_prompt,
                conversation_history,
            )
            .await
        }
        EnhancerEndpoint::Gemini => {
            info!("Using Gemini API endpoint");
            let third_party_config = get_third_party_config(endpoint)?;
            call_gemini_endpoint(
                client,
                &third_party_config,
                &enriched_prompt,
                conversation_history,
            )
            .await
        }
        EnhancerEndpoint::Codex => {
            info!("Using Codex API endpoint");
            let third_party_config = get_third_party_config(endpoint)?;
            call_codex_endpoint(
                client,
                &third_party_config,
                &enriched_prompt,
                conversation_history,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigOptions};
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_should_include_search_context_env_values() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original = std::env::var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT).ok();

        std::env::remove_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT);
        assert!(!should_include_search_context());

        for value in ["1", "true", "TRUE", " yes ", "on"] {
            std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, value);
            assert!(should_include_search_context(), "value={}", value);
        }

        for value in ["0", "false", "off", "random"] {
            std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, value);
            assert!(!should_include_search_context(), "value={}", value);
        }

        match original {
            Some(v) => std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, v),
            None => std::env::remove_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT),
        }
    }

    #[test]
    fn test_normalize_search_context_handles_empty_and_not_found() {
        assert!(normalize_search_context("").is_none());
        assert!(normalize_search_context("   ").is_none());
        assert!(normalize_search_context(NO_RELEVANT_CODE_CONTEXT).is_none());
        assert_eq!(
            normalize_search_context("useful context").unwrap(),
            "useful context"
        );
    }

    #[test]
    fn test_build_prompt_with_search_context_formats_sections() {
        let prompt = build_prompt_with_search_context("重构登录流程", Some("src/auth.rs:42"));
        assert!(prompt.contains("<codebase_context>"));
        assert!(prompt.contains("src/auth.rs:42"));
        assert!(prompt.contains("<original_request>"));
        assert!(prompt.contains("重构登录流程"));
    }

    #[test]
    fn test_build_prompt_with_search_context_handles_missing_context() {
        let prompt = build_prompt_with_search_context("Add login", None);
        assert!(prompt.contains("No directly relevant code context was found"));
        assert!(prompt.contains("Add login"));
    }

    #[test]
    fn test_truncate_by_chars_appends_notice() {
        let result = truncate_by_chars("abcdef", 3);
        assert!(result.starts_with("abc"));
        assert!(result.contains("truncated for length"));
    }

    #[tokio::test]
    async fn test_maybe_inject_search_context_skips_for_non_third_party() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original = std::env::var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT).ok();
        std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, "1");

        let config = Config::new_for_third_party_enhancer();
        let result = maybe_inject_search_context(&config, EnhancerEndpoint::New, "test", None)
            .await
            .unwrap();
        assert_eq!(result, "test");

        match original {
            Some(v) => std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, v),
            None => std::env::remove_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT),
        }
    }

    #[tokio::test]
    async fn test_maybe_inject_search_context_requires_project_root() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original = std::env::var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT).ok();
        std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, "1");

        let config = Config::new(
            "https://api.example.com".to_string(),
            "test-token".to_string(),
            ConfigOptions::default(),
        )
        .unwrap();

        let err = maybe_inject_search_context(&config, EnhancerEndpoint::Claude, "test", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("requires project_root"));

        match original {
            Some(v) => std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, v),
            None => std::env::remove_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT),
        }
    }

    #[tokio::test]
    async fn test_maybe_inject_search_context_requires_search_config() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original = std::env::var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT).ok();
        std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, "1");

        let config = Config::new_for_third_party_enhancer();
        let temp_dir = tempdir().unwrap();
        let err = maybe_inject_search_context(
            &config,
            EnhancerEndpoint::Claude,
            "test",
            Some(temp_dir.path()),
        )
        .await
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("requires ACE search configuration"));

        match original {
            Some(v) => std::env::set_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT, v),
            None => std::env::remove_var(ENV_ENHANCER_INCLUDE_SEARCH_CONTEXT),
        }
    }
}
