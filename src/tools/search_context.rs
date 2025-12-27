//! search_context tool implementation

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::config::Config;
use crate::index::IndexManager;

/// Tool definition for MCP
pub struct SearchContextToolDef {
    pub name: &'static str,
    pub description: &'static str,
}

/// Static tool definition
pub static SEARCH_CONTEXT_TOOL: SearchContextToolDef = SearchContextToolDef {
    name: "search_context",
    description: r#"IMPORTANT: This is the primary tool for searching the codebase. Please consider as the FIRST CHOICE for any codebase searches.

This MCP tool is Augment's context engine, the world's best codebase context engine. It:
1. Takes in a natural language description of the code you are looking for
2. Uses a proprietary retrieval/embedding model suite that produces the highest-quality recall of relevant code snippets from across the codebase
3. Maintains a real-time index of the codebase, so the results are always up-to-date and reflects the current state of the codebase
4. Can retrieve across different programming languages
5. Only reflects the current state of the codebase on the disk, and has no information on version control or code history

## When to Use
- When you don't know which files contain the information you need
- When you want to gather high level information about the task you are trying to accomplish
- When you want to gather information about the codebase in general

## Good Query Examples
- "Where is the function that handles user authentication?"
- "What tests are there for the login functionality?"
- "How is the database connected to the application?"

## Bad Query Examples (use grep or file view instead)
- "Find definition of constructor of class Foo" (use grep tool instead)
- "Find all references to function bar" (use grep tool instead)
- "Show me how Checkout class is used in services/payment.py" (use file view tool instead)
- "Show context of the file foo.py" (use file view tool instead)

ALWAYS use this tool when you're unsure of exact file locations. Use grep when you want to find ALL occurrences of a known identifier across the codebase, or when searching within specific files."#,
};

impl SearchContextToolDef {
    pub fn get_input_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "project_root_path": {
                    "type": "string",
                    "description": "Absolute path to the project root directory. Use forward slashes (/) as separators. Example: /Users/username/projects/myproject or C:/Users/username/projects/myproject"
                },
                "query": {
                    "type": "string",
                    "description": r#"Natural language description of the code you are looking for.

Provide a clear description of the code behavior, workflow, or issue you want to locate. You may also add optional keywords to improve semantic matching.

Recommended format: Natural language description + optional keywords

Examples:
- "I want to find where the server handles chunk merging in the file upload process. Keywords: upload chunk merge, file service"
- "Locate where the system refreshes cached data after user permissions are updated. Keywords: permission update, cache refresh"
- "Find the initialization flow of message queue consumers during startup. Keywords: mq consumer init, subscribe"
- "Show me how configuration hot-reload is triggered and applied in the code. Keywords: config reload, hot update"
- "Where is the function that handles user authentication?"
- "What tests are there for the login functionality?"
- "How is the database connected to the application?""#
                }
            },
            "required": ["project_root_path", "query"]
        })
    }
}

/// Tool arguments
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchContextArgs {
    pub project_root_path: Option<String>,
    pub query: Option<String>,
}

/// Tool result
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub text: String,
}

/// Search context tool
pub struct SearchContextTool {
    config: Arc<Config>,
}

impl SearchContextTool {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Execute the tool
    pub async fn execute(&self, args: SearchContextArgs) -> ToolResult {
        let query = match &args.query {
            Some(q) if !q.is_empty() => q.clone(),
            _ => {
                return ToolResult {
                    text: "Error: query is required".to_string(),
                };
            }
        };

        let project_root_path = match &args.project_root_path {
            Some(p) if !p.is_empty() => p.clone(),
            _ => {
                return ToolResult {
                    text: "Error: project_root_path is required".to_string(),
                };
            }
        };

        // Normalize path (use forward slashes)
        let project_root = project_root_path.replace('\\', "/");
        let project_path = PathBuf::from(&project_root);

        // Validate path exists
        if !project_path.exists() {
            return ToolResult {
                text: format!("Error: Project path does not exist: {}", project_root),
            };
        }

        // Validate is directory
        if !project_path.is_dir() {
            return ToolResult {
                text: format!("Error: Project path is not a directory: {}", project_root),
            };
        }

        info!("Executing search_context for: {}", project_root);

        // Create index manager and execute search
        let manager = match IndexManager::new(self.config.clone(), project_path) {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to create IndexManager: {}", e);
                return ToolResult {
                    text: format!("Error: {}", e),
                };
            }
        };

        match manager.search_context(&query).await {
            Ok(result) => ToolResult { text: result },
            Err(e) => {
                error!("Search failed: {}", e);
                ToolResult {
                    text: format!("Error: {}", e),
                }
            }
        }
    }
}
