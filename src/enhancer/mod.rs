//! Prompt Enhancer module
//! Enhances user prompts using codebase context and conversation history

pub mod prompt_enhancer;
pub mod server;
pub mod templates;

pub use prompt_enhancer::PromptEnhancer;
pub use server::EnhancerServer;
pub use templates::{ENHANCER_UI_HTML, ENHANCE_PROMPT_TEMPLATE};
