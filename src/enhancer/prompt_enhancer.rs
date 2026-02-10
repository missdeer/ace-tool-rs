//! Prompt Enhancer - Core enhancement logic
//! Based on Augment VSCode plugin implementation
//!
//! Supports multiple API endpoints controlled by environment variable `ACE_ENHANCER_ENDPOINT`:
//! - `new`: Uses Augment /prompt-enhancer endpoint (default)
//! - `old`: Uses Augment /chat-stream endpoint
//! - `claude`: Uses Claude API (Anthropic)
//! - `openai`: Uses OpenAI API
//! - `gemini`: Uses Gemini API (Google)

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::service::{
    call_claude_endpoint, call_gemini_endpoint, call_new_endpoint, call_old_endpoint,
    call_openai_endpoint, get_third_party_config, EnhancerEndpoint,
};
use crate::utils::project_detector::get_index_file_path;

use super::server::EnhancerServer;

/// Singleton EnhancerServer shared across all PromptEnhancer instances to prevent port leaks.
/// Each `EnhancerServer::start()` binds a new port (3000-3099); without sharing, repeated
/// `enhance_prompt` calls exhaust all available ports.
static SHARED_SERVER: OnceLock<Arc<EnhancerServer>> = OnceLock::new();

/// Environment variable to control which endpoint to use
pub const ENV_ENHANCER_ENDPOINT: &str = "ACE_ENHANCER_ENDPOINT";

/// Get the configured enhancer endpoint type
pub fn get_enhancer_endpoint() -> EnhancerEndpoint {
    std::env::var(ENV_ENHANCER_ENDPOINT)
        .map(|v| EnhancerEndpoint::from_env_str(&v))
        .unwrap_or(EnhancerEndpoint::New)
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
            .call_prompt_enhancer_api(original_prompt, conversation_history, &blob_names)
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
) -> Result<String> {
    let endpoint = get_enhancer_endpoint();

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
                original_prompt,
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
                original_prompt,
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
                original_prompt,
                conversation_history,
            )
            .await
        }
    }
}
