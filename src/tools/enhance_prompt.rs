//! enhance_prompt tool implementation

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::config::Config;
use crate::enhancer::PromptEnhancer;

/// Tool definition for MCP
pub struct EnhancePromptToolDef {
    pub name: &'static str,
    pub description: &'static str,
}

/// Static tool definition
pub static ENHANCE_PROMPT_TOOL: EnhancePromptToolDef = EnhancePromptToolDef {
    name: "enhance_prompt",
    description: r#"Enhances user requirements by combining codebase context and conversation history to generate clearer, more specific, and actionable prompts.

IMPORTANT: Use this tool ONLY when:
(1) User message contains explicit markers: -enhance, -enhancer, -Enhance, -Enhancer (case-insensitive, can appear anywhere in message).
    Examples: "新加一个登录页面-Enhancer", "Add login feature -enhance"
(2) User explicitly asks to "enhance my prompt" or "use enhance_prompt tool".

DO NOT use for general optimization requests like "optimize this code" or "improve this function" - those are code optimization requests, not prompt enhancement.

Features:
- Automatic language detection (Chinese input → Chinese output, English input → English output)
- Uses codebase context from indexed files
- Considers conversation history for better context understanding

Supports English and Chinese."#,
};

impl EnhancePromptToolDef {
    pub fn get_input_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "project_root_path": {
                    "type": "string",
                    "description": "Absolute path to the project root directory (optional, defaults to current working directory)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The original prompt to enhance"
                },
                "conversation_history": {
                    "type": "string",
                    "description": "Recent conversation history (5-10 rounds) to help understand user intent and project context. Format: 'User: xxx\\nAssistant: yyy'"
                }
            },
            "required": ["prompt", "conversation_history"]
        })
    }
}

/// Tool arguments
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnhancePromptArgs {
    pub project_root_path: Option<String>,
    pub prompt: Option<String>,
    pub conversation_history: Option<String>,
}

/// Tool result
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub text: String,
}

/// Enhance prompt tool
pub struct EnhancePromptTool {
    config: Arc<Config>,
}

impl EnhancePromptTool {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Execute the tool
    pub async fn execute(&self, args: EnhancePromptArgs) -> ToolResult {
        let prompt = match &args.prompt {
            Some(p) if !p.is_empty() => p.clone(),
            _ => {
                return ToolResult {
                    text: "Error: prompt is required".to_string(),
                };
            }
        };

        let conversation_history = match &args.conversation_history {
            Some(h) => h.clone(),
            None => String::new(),
        };

        // Determine project root
        let project_root = args
            .project_root_path
            .as_ref()
            .map(|p| PathBuf::from(p.replace('\\', "/")));

        info!("Executing enhance_prompt");
        if let Some(ref root) = project_root {
            info!("Project path: {:?}", root);
        }

        // Create enhancer and execute
        let enhancer = match PromptEnhancer::new(self.config.clone()) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to create PromptEnhancer: {}", e);
                return ToolResult {
                    text: format!("Error: {}", e),
                };
            }
        };

        let result = if self.config.no_webbrowser_enhance_prompt {
            enhancer
                .enhance_simple(&prompt, &conversation_history, project_root.as_deref())
                .await
        } else {
            enhancer
                .enhance(&prompt, &conversation_history, project_root.as_deref())
                .await
        };

        match result {
            Ok(enhanced) => ToolResult { text: enhanced },
            Err(e) => {
                error!("Enhancement failed: {}", e);
                ToolResult {
                    text: format!("Error: {}", e),
                }
            }
        }
    }
}
