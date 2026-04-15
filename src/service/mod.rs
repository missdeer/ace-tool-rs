//! Service modules for different API providers

pub(crate) mod augment;
pub(crate) mod claude;
pub(crate) mod codex;
pub mod common;
pub(crate) mod gemini;
pub(crate) mod openai;

// Re-export commonly used items
pub use augment::{
    call_new_endpoint, call_old_endpoint, parse_streaming_response, DEFAULT_MODEL, NODE_ID_NEW,
    NODE_ID_OLD,
};
pub use claude::call_claude_endpoint;
pub use codex::call_codex_endpoint;
pub use common::{
    build_api_url, extract_enhanced_prompt, get_third_party_config, is_chinese_text,
    parse_chat_history, render_enhance_prompt, replace_tool_names, ChatMessage, EnhancerEndpoint,
    ThirdPartyConfig, DEFAULT_CLAUDE_MODEL, DEFAULT_CODEX_MODEL, DEFAULT_GEMINI_MODEL,
    DEFAULT_OPENAI_MODEL, ENV_ENHANCER_BASE_URL, ENV_ENHANCER_MODEL, ENV_ENHANCER_TOKEN,
};
pub use gemini::call_gemini_endpoint;
pub use openai::call_openai_endpoint;
