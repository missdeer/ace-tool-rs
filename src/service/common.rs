//! Common types and utilities for service modules

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::enhancer::templates::ENHANCE_PROMPT_TEMPLATE;

/// Environment variable for custom prompt enhancer base URL
pub const ENV_ENHANCER_BASE_URL: &str = "PROMPT_ENHANCER_BASE_URL";

/// Environment variable for custom prompt enhancer auth token
pub const ENV_ENHANCER_TOKEN: &str = "PROMPT_ENHANCER_TOKEN";

/// Environment variable for custom prompt enhancer model
pub const ENV_ENHANCER_MODEL: &str = "PROMPT_ENHANCER_MODEL";

/// Default models for third-party APIs
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.0-flash-exp";

/// Enhancer endpoint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnhancerEndpoint {
    /// Use Augment /prompt-enhancer endpoint (default)
    New,
    /// Use Augment /chat-stream endpoint
    Old,
    /// Use Claude API (Anthropic)
    Claude,
    /// Use OpenAI API
    OpenAI,
    /// Use Gemini API (Google)
    Gemini,
}

impl std::fmt::Display for EnhancerEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => write!(f, "new"),
            Self::Old => write!(f, "old"),
            Self::Claude => write!(f, "claude"),
            Self::OpenAI => write!(f, "openai"),
            Self::Gemini => write!(f, "gemini"),
        }
    }
}

impl EnhancerEndpoint {
    /// Parse from environment variable string
    pub fn from_env_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "old" => Self::Old,
            "claude" => Self::Claude,
            "openai" => Self::OpenAI,
            "gemini" => Self::Gemini,
            _ => Self::New, // default
        }
    }

    /// Check if this is a third-party API (Claude/OpenAI/Gemini)
    pub fn is_third_party(&self) -> bool {
        matches!(self, Self::Claude | Self::OpenAI | Self::Gemini)
    }
}

/// Configuration for third-party API endpoints
#[derive(Debug, Clone)]
pub struct ThirdPartyConfig {
    pub base_url: String,
    pub token: String,
    pub model: String,
}

/// Get third-party API configuration from environment variables
pub fn get_third_party_config(endpoint: EnhancerEndpoint) -> Result<ThirdPartyConfig> {
    let base_url = std::env::var(ENV_ENHANCER_BASE_URL).map_err(|_| {
        anyhow!(
            "{} environment variable is required for '{}' endpoint",
            ENV_ENHANCER_BASE_URL,
            endpoint
        )
    })?;

    let token = std::env::var(ENV_ENHANCER_TOKEN).map_err(|_| {
        anyhow!(
            "{} environment variable is required for '{}' endpoint",
            ENV_ENHANCER_TOKEN,
            endpoint
        )
    })?;

    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err(anyhow!(
            "{} environment variable is required for '{}' endpoint",
            ENV_ENHANCER_BASE_URL,
            endpoint
        ));
    }

    let token = token.trim();
    if token.is_empty() {
        return Err(anyhow!(
            "{} environment variable is required for '{}' endpoint",
            ENV_ENHANCER_TOKEN,
            endpoint
        ));
    }

    let default_model = match endpoint {
        EnhancerEndpoint::Claude => DEFAULT_CLAUDE_MODEL,
        EnhancerEndpoint::OpenAI => DEFAULT_OPENAI_MODEL,
        EnhancerEndpoint::Gemini => DEFAULT_GEMINI_MODEL,
        _ => "claude-sonnet-4-5",
    };

    let model = match std::env::var(ENV_ENHANCER_MODEL) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                default_model.to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => default_model.to_string(),
    };

    // Normalize base URL
    let base_url = base_url.trim_end_matches('/').to_string();

    Ok(ThirdPartyConfig {
        base_url,
        token: token.to_string(),
        model,
    })
}

/// Chat message for conversation history
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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

/// Extract enhanced prompt from XML-like response
/// Looks for content between <augment-enhanced-prompt> and </augment-enhanced-prompt> tags
pub fn extract_enhanced_prompt(text: &str) -> Option<String> {
    lazy_static::lazy_static! {
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
pub fn is_chinese_text(text: &str) -> bool {
    lazy_static::lazy_static! {
        static ref CHINESE_RE: Regex = Regex::new(r"[\u4e00-\u9fa5]").unwrap();
    }

    let chinese_count = CHINESE_RE.find_iter(text).count();
    if chinese_count == 0 {
        return false;
    }

    if chinese_count >= 3 {
        return true;
    }

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

/// Render the enhance prompt template safely without corrupting user input
/// Uses split+concat instead of replace to avoid replacing placeholders
/// that may appear in user content
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

/// Build the full prompt for third-party APIs using the template
pub fn build_third_party_prompt(original_prompt: &str) -> Result<String> {
    let enhanced_prompt = render_enhance_prompt(original_prompt)?;

    let language_hint = if is_chinese_text(original_prompt) {
        "\n\n请用中文回复。"
    } else {
        ""
    };

    Ok(format!("{}{}", enhanced_prompt, language_hint))
}

/// Map common authentication errors to consistent error messages
pub fn map_auth_error(status: u16, provider: &str) -> Option<anyhow::Error> {
    match status {
        401 => Some(anyhow!("{} API key invalid or expired", provider)),
        403 => Some(anyhow!(
            "{} access denied, API key may be disabled",
            provider
        )),
        _ => None,
    }
}

/// Lazy static macro for regex
pub mod lazy_static {
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
