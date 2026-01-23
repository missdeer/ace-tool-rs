//! Tests for tools module

use std::sync::Arc;
use tempfile::TempDir;

use ace_tool::config::{Config, ConfigOptions};
use ace_tool::tools::search_context::{
    SearchContextArgs, SearchContextTool, SearchContextToolDef, ToolResult, SEARCH_CONTEXT_TOOL,
};

fn create_test_config() -> Arc<Config> {
    Config::new(
        "https://api.example.com".to_string(),
        "test-token".to_string(),
        ConfigOptions::default(),
    )
    .unwrap()
}

#[test]
fn test_search_context_tool_def() {
    assert_eq!(SEARCH_CONTEXT_TOOL.name, "search_context");
    assert!(SEARCH_CONTEXT_TOOL.description.contains("primary tool"));
    assert!(SEARCH_CONTEXT_TOOL.description.contains("codebase"));
}

#[test]
fn test_get_input_schema() {
    let schema = SearchContextToolDef::get_input_schema();

    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["project_root_path"].is_object());
    assert!(schema["properties"]["query"].is_object());
    assert_eq!(schema["required"][0], "project_root_path");
    assert_eq!(schema["required"][1], "query");
}

#[test]
fn test_search_context_args_default() {
    let args = SearchContextArgs::default();
    assert!(args.project_root_path.is_none());
    assert!(args.query.is_none());
}

#[test]
fn test_search_context_args_serialization() {
    let args = SearchContextArgs {
        project_root_path: Some("/path/to/project".to_string()),
        query: Some("find authentication".to_string()),
    };

    let json = serde_json::to_string(&args).unwrap();
    assert!(json.contains("/path/to/project"));
    assert!(json.contains("find authentication"));

    let deserialized: SearchContextArgs = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.project_root_path, args.project_root_path);
    assert_eq!(deserialized.query, args.query);
}

#[test]
fn test_tool_result() {
    let result = ToolResult {
        text: "Found some code".to_string(),
    };
    assert_eq!(result.text, "Found some code");
}

#[test]
fn test_search_context_tool_new() {
    let config = create_test_config();
    let _tool = SearchContextTool::new(config);
}

#[tokio::test]
async fn test_execute_missing_query() {
    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some("/some/path".to_string()),
        query: None,
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("query is required"));
}

#[tokio::test]
async fn test_execute_empty_query() {
    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some("/some/path".to_string()),
        query: Some("".to_string()),
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("query is required"));
}

#[tokio::test]
async fn test_execute_missing_project_path() {
    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: None,
        query: Some("find something".to_string()),
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("project_root_path is required"));
}

#[tokio::test]
async fn test_execute_empty_project_path() {
    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some("".to_string()),
        query: Some("find something".to_string()),
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("project_root_path is required"));
}

#[tokio::test]
async fn test_execute_nonexistent_path() {
    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some("/nonexistent/path/that/does/not/exist".to_string()),
        query: Some("find something".to_string()),
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("does not exist"));
}

#[tokio::test]
async fn test_execute_path_is_file_not_directory() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, "test content").unwrap();

    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some(file_path.to_string_lossy().to_string()),
        query: Some("find something".to_string()),
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("not a directory"));
}
