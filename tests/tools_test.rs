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
    assert!(args.exclude_document_files.is_none());
    assert!(args.exclude_extensions.is_none());
    assert!(args.exclude_globs.is_none());
}

#[test]
fn test_search_context_args_serialization() {
    let args = SearchContextArgs {
        project_root_path: Some("/path/to/project".to_string()),
        query: Some("find authentication".to_string()),
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
    };

    let result = tool.execute(args).await;
    assert!(result.text.contains("Error"));
    assert!(result.text.contains("not a directory"));
}

#[test]
fn test_search_context_args_with_filters() {
    let args = SearchContextArgs {
        project_root_path: Some("/path/to/project".to_string()),
        query: Some("find code".to_string()),
        exclude_document_files: Some(true),
        exclude_extensions: Some(vec![".md".to_string(), ".txt".to_string()]),
        exclude_globs: Some(vec!["docs/**".to_string()]),
    };

    let json = serde_json::to_string(&args).unwrap();
    assert!(json.contains("exclude_document_files"));
    assert!(json.contains("exclude_extensions"));
    assert!(json.contains("exclude_globs"));

    let deserialized: SearchContextArgs = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.exclude_document_files, Some(true));
    assert_eq!(
        deserialized.exclude_extensions,
        Some(vec![".md".to_string(), ".txt".to_string()])
    );
    assert_eq!(
        deserialized.exclude_globs,
        Some(vec!["docs/**".to_string()])
    );
}

#[test]
fn test_search_context_args_backward_compatible() {
    // Old format without new fields should still work
    let json = r#"{"project_root_path":"/path","query":"test"}"#;
    let args: SearchContextArgs = serde_json::from_str(json).unwrap();
    assert_eq!(args.project_root_path, Some("/path".to_string()));
    assert_eq!(args.query, Some("test".to_string()));
    assert!(args.exclude_document_files.is_none());
    assert!(args.exclude_extensions.is_none());
    assert!(args.exclude_globs.is_none());
}

#[test]
fn test_get_input_schema_includes_filters() {
    let schema = SearchContextToolDef::get_input_schema();

    // Check new fields exist in schema
    assert!(schema["properties"]["exclude_document_files"].is_object());
    assert_eq!(
        schema["properties"]["exclude_document_files"]["type"],
        "boolean"
    );

    assert!(schema["properties"]["exclude_extensions"].is_object());
    assert_eq!(schema["properties"]["exclude_extensions"]["type"], "array");

    assert!(schema["properties"]["exclude_globs"].is_object());
    assert_eq!(schema["properties"]["exclude_globs"]["type"], "array");

    // New fields should NOT be required (backward compatible)
    let required = schema["required"].as_array().unwrap();
    assert!(!required.iter().any(|r| r == "exclude_document_files"));
    assert!(!required.iter().any(|r| r == "exclude_extensions"));
    assert!(!required.iter().any(|r| r == "exclude_globs"));
}

#[test]
fn test_from_args_injects_default_document_extensions() {
    use ace_tool::search_filter::SearchFilterOptions;
    use ace_tool::tools::search_context::SearchContextArgs;

    let args = SearchContextArgs {
        project_root_path: Some("/path".to_string()),
        query: Some("test".to_string()),
        exclude_document_files: Some(true),
        exclude_extensions: None,
        exclude_globs: None,
    };

    let filter = SearchFilterOptions::from_args(&args);

    // 验证默认文档扩展名被注入
    assert!(filter.exclude_extensions.contains(".md"));
    assert!(filter.exclude_extensions.contains(".mdx"));
    assert!(filter.exclude_extensions.contains(".txt"));
    assert!(filter.exclude_extensions.contains(".csv"));
    assert!(filter.exclude_extensions.contains(".tsv"));
    assert!(filter.exclude_extensions.contains(".rst"));
    assert!(filter.exclude_extensions.contains(".adoc"));
    assert!(filter.exclude_extensions.contains(".tex"));
    assert!(filter.exclude_extensions.contains(".org"));

    // 验证 is_active() 返回 true
    assert!(filter.is_active());
}

#[test]
fn test_invalid_glob_pattern_error_handling() {
    use ace_tool::search_filter::SearchFilterOptions;
    use ace_tool::tools::search_context::SearchContextArgs;

    let args = SearchContextArgs {
        project_root_path: Some("/path".to_string()),
        query: Some("test".to_string()),
        exclude_document_files: None,
        exclude_extensions: None,
        exclude_globs: Some(vec!["[".to_string()]), // 无效 glob 模式
    };

    let mut filter = SearchFilterOptions::from_args(&args);
    let result = filter.compile_globs();

    // 验证编译失败
    assert!(result.is_err());

    // 验证错误消息包含模式信息
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("["));
}

#[tokio::test]
async fn test_execute_invalid_glob_returns_error_text() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().to_str().unwrap().to_string();

    let config = create_test_config();
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some(project_path),
        query: Some("test query".to_string()),
        exclude_document_files: None,
        exclude_extensions: None,
        exclude_globs: Some(vec!["[".to_string()]), // 无效 glob
    };

    let result = tool.execute(args).await;

    // 验证返回的错误文本格式
    assert!(result.text.starts_with("Error:"));
    assert!(result.text.contains("Invalid glob pattern"));
}
