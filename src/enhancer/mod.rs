//! Prompt Enhancer module
//! Enhances user prompts using codebase context and conversation history

mod prompt_enhancer;
mod server;
mod templates;

pub use prompt_enhancer::PromptEnhancer;
pub use server::EnhancerServer;
pub use templates::ENHANCER_UI_HTML;
