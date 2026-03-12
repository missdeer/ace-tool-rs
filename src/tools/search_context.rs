//! search_context tool implementation

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::config::Config;
use crate::index::IndexManager;
use crate::search_filter::SearchFilterOptions;

/// Tool definition for MCP
pub struct SearchContextToolDef {
    pub name: &'static str,
    pub description: &'static str,
}

/// Static tool definition
pub static SEARCH_CONTEXT_TOOL: SearchContextToolDef = SearchContextToolDef {
    name: "search_context",
    description: r#"Semantic code search tool. Use as FIRST CHOICE for codebase searches.

Takes natural language queries and returns relevant code snippets using semantic matching. Maintains real-time index of the codebase.

## When to Use
- Don't know which files contain the information
- Gathering high-level information about codebase
- Looking for implementation of a feature

## Query Examples
Good: "Where is user authentication handled?" "How is DB connection pool managed?"
Bad (use grep): "Find definition of class Foo" "Find all references to bar"

## Filtering Options (optional)
- exclude_document_files (bool): RECOMMENDED DEFAULT. Set to `true` to search source code only, excluding .md, .txt, README, CHANGELOG, etc. Only set to `false` when you specifically need to search documentation.
- exclude_extensions (array): Exclude extensions, e.g., [".md", ".json"]
- exclude_globs (array): Exclude glob patterns, e.g., ["docs/**", "**/test*"]

Filters combine as UNION (OR logic).

Use grep for: exact symbol definitions, all references, specific file content."#,
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
                },
                "exclude_document_files": {
                    "type": "boolean",
                    "description": "If true, exclude document files (.md, .mdx, .txt, .csv, .tsv, .rst, .adoc, .tex, .org) from search. Useful when you want to focus on source code only."
                },
                "exclude_extensions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File extensions to exclude from search. Include the leading dot, e.g., [\".md\", \".txt\"]. Case-insensitive matching."
                },
                "exclude_globs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Glob patterns to exclude from search. Examples: [\"docs/**\", \"**/README*\", \"test/**\"]. Uses standard glob syntax."
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
    /// Whether to exclude document files (md, txt, csv, etc.) from search
    pub exclude_document_files: Option<bool>,
    /// File extensions to exclude (e.g., [".md", ".txt"])
    pub exclude_extensions: Option<Vec<String>>,
    /// Glob patterns to exclude (e.g., ["docs/**", "**/README*"])
    pub exclude_globs: Option<Vec<String>>,
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

        // Build filter options from args
        let mut filters = SearchFilterOptions::from_args(&args);
        if let Err(e) = filters.compile_globs() {
            return ToolResult {
                text: format!("Error: Invalid glob pattern: {}", e),
            };
        }

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

        match manager.search_context(&query, &filters).await {
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
