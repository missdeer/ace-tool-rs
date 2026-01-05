//! Tests for enhance_prompt tool

use ace_tool::tools::enhance_prompt::{
    EnhancePromptArgs, EnhancePromptToolDef, ENHANCE_PROMPT_TOOL,
};

// ============================================================================
// Tool Definition Tests
// ============================================================================

#[test]
fn test_enhance_prompt_tool_name() {
    assert_eq!(ENHANCE_PROMPT_TOOL.name, "enhance_prompt");
}

#[test]
fn test_enhance_prompt_tool_description_not_empty() {
    assert!(!ENHANCE_PROMPT_TOOL.description.is_empty());
}

#[test]
fn test_enhance_prompt_tool_description_contains_key_info() {
    let desc = ENHANCE_PROMPT_TOOL.description;
    assert!(desc.contains("enhance"));
    assert!(desc.contains("-enhance"));
    assert!(desc.contains("-enhancer"));
}

#[test]
fn test_enhance_prompt_tool_description_mentions_language_detection() {
    let desc = ENHANCE_PROMPT_TOOL.description;
    assert!(desc.contains("Chinese") || desc.contains("language"));
}

// ============================================================================
// Input Schema Tests
// ============================================================================

#[test]
fn test_get_input_schema_returns_object() {
    let schema = EnhancePromptToolDef::get_input_schema();
    assert_eq!(schema["type"], "object");
}

#[test]
fn test_get_input_schema_has_properties() {
    let schema = EnhancePromptToolDef::get_input_schema();
    assert!(schema["properties"].is_object());
}

#[test]
fn test_get_input_schema_has_project_root_path() {
    let schema = EnhancePromptToolDef::get_input_schema();
    assert!(schema["properties"]["project_root_path"].is_object());
    assert_eq!(schema["properties"]["project_root_path"]["type"], "string");
}

#[test]
fn test_get_input_schema_has_prompt() {
    let schema = EnhancePromptToolDef::get_input_schema();
    assert!(schema["properties"]["prompt"].is_object());
    assert_eq!(schema["properties"]["prompt"]["type"], "string");
}

#[test]
fn test_get_input_schema_has_conversation_history() {
    let schema = EnhancePromptToolDef::get_input_schema();
    assert!(schema["properties"]["conversation_history"].is_object());
    assert_eq!(
        schema["properties"]["conversation_history"]["type"],
        "string"
    );
}

#[test]
fn test_get_input_schema_required_fields() {
    let schema = EnhancePromptToolDef::get_input_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "prompt"));
    assert!(required.iter().any(|v| v == "conversation_history"));
}

#[test]
fn test_get_input_schema_project_root_not_required() {
    let schema = EnhancePromptToolDef::get_input_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(!required.iter().any(|v| v == "project_root_path"));
}

// ============================================================================
// EnhancePromptArgs Tests
// ============================================================================

#[test]
fn test_enhance_prompt_args_default() {
    let args = EnhancePromptArgs::default();
    assert!(args.project_root_path.is_none());
    assert!(args.prompt.is_none());
    assert!(args.conversation_history.is_none());
}

#[test]
fn test_enhance_prompt_args_serialization() {
    let args = EnhancePromptArgs {
        project_root_path: Some("/path/to/project".to_string()),
        prompt: Some("Add login feature".to_string()),
        conversation_history: Some("User: Hello\nAssistant: Hi".to_string()),
    };

    let json = serde_json::to_string(&args).unwrap();
    assert!(json.contains("project_root_path"));
    assert!(json.contains("prompt"));
    assert!(json.contains("conversation_history"));
}

#[test]
fn test_enhance_prompt_args_deserialization() {
    let json = r#"{
        "project_root_path": "/test/path",
        "prompt": "test prompt",
        "conversation_history": "User: test"
    }"#;

    let args: EnhancePromptArgs = serde_json::from_str(json).unwrap();
    assert_eq!(args.project_root_path, Some("/test/path".to_string()));
    assert_eq!(args.prompt, Some("test prompt".to_string()));
    assert_eq!(args.conversation_history, Some("User: test".to_string()));
}

#[test]
fn test_enhance_prompt_args_deserialization_partial() {
    let json = r#"{
        "prompt": "test prompt"
    }"#;

    let args: EnhancePromptArgs = serde_json::from_str(json).unwrap();
    assert!(args.project_root_path.is_none());
    assert_eq!(args.prompt, Some("test prompt".to_string()));
    assert!(args.conversation_history.is_none());
}

#[test]
fn test_enhance_prompt_args_deserialization_empty_object() {
    let json = "{}";
    let args: EnhancePromptArgs = serde_json::from_str(json).unwrap();
    assert!(args.project_root_path.is_none());
    assert!(args.prompt.is_none());
    assert!(args.conversation_history.is_none());
}

#[test]
fn test_enhance_prompt_args_with_unicode() {
    let args = EnhancePromptArgs {
        project_root_path: Some("/路径/项目".to_string()),
        prompt: Some("添加登录功能".to_string()),
        conversation_history: Some("用户: 你好\n助手: 你好！".to_string()),
    };

    let json = serde_json::to_string(&args).unwrap();
    let parsed: EnhancePromptArgs = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.project_root_path, args.project_root_path);
    assert_eq!(parsed.prompt, args.prompt);
    assert_eq!(parsed.conversation_history, args.conversation_history);
}

#[test]
fn test_enhance_prompt_args_with_special_characters() {
    let args = EnhancePromptArgs {
        project_root_path: Some("/path/with spaces/project".to_string()),
        prompt: Some("Add feature with \"quotes\" and 'apostrophes'".to_string()),
        conversation_history: Some("User: Line1\nLine2\tTabbed".to_string()),
    };

    let json = serde_json::to_string(&args).unwrap();
    let parsed: EnhancePromptArgs = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.prompt, args.prompt);
}

#[test]
fn test_enhance_prompt_args_clone() {
    let args = EnhancePromptArgs {
        project_root_path: Some("/test".to_string()),
        prompt: Some("test".to_string()),
        conversation_history: Some("history".to_string()),
    };

    let cloned = args.clone();
    assert_eq!(cloned.project_root_path, args.project_root_path);
    assert_eq!(cloned.prompt, args.prompt);
    assert_eq!(cloned.conversation_history, args.conversation_history);
}

#[test]
fn test_enhance_prompt_args_debug() {
    let args = EnhancePromptArgs {
        project_root_path: Some("/test".to_string()),
        prompt: Some("test".to_string()),
        conversation_history: None,
    };

    let debug_str = format!("{:?}", args);
    assert!(debug_str.contains("EnhancePromptArgs"));
    assert!(debug_str.contains("/test"));
}

// ============================================================================
// Windows Path Handling Tests
// ============================================================================

#[test]
fn test_enhance_prompt_args_with_windows_path() {
    let args = EnhancePromptArgs {
        project_root_path: Some("C:\\Users\\test\\project".to_string()),
        prompt: Some("test".to_string()),
        conversation_history: None,
    };

    let json = serde_json::to_string(&args).unwrap();
    let parsed: EnhancePromptArgs = serde_json::from_str(&json).unwrap();

    assert_eq!(
        parsed.project_root_path,
        Some("C:\\Users\\test\\project".to_string())
    );
}

#[test]
fn test_enhance_prompt_args_with_mixed_path_separators() {
    let args = EnhancePromptArgs {
        project_root_path: Some("C:/Users\\test/project".to_string()),
        prompt: Some("test".to_string()),
        conversation_history: None,
    };

    assert!(args.project_root_path.is_some());
}

// ============================================================================
// Large Input Tests
// ============================================================================

#[test]
fn test_enhance_prompt_args_with_large_prompt() {
    let large_prompt = "x".repeat(10000);
    let args = EnhancePromptArgs {
        project_root_path: None,
        prompt: Some(large_prompt.clone()),
        conversation_history: None,
    };

    let json = serde_json::to_string(&args).unwrap();
    let parsed: EnhancePromptArgs = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.prompt.unwrap().len(), 10000);
}

#[test]
fn test_enhance_prompt_args_with_large_conversation_history() {
    let large_history = (0..100)
        .map(|i| format!("User: Message {}\nAssistant: Response {}", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    let args = EnhancePromptArgs {
        project_root_path: None,
        prompt: Some("test".to_string()),
        conversation_history: Some(large_history.clone()),
    };

    let json = serde_json::to_string(&args).unwrap();
    let parsed: EnhancePromptArgs = serde_json::from_str(&json).unwrap();

    assert!(parsed.conversation_history.unwrap().contains("Message 99"));
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_enhance_prompt_args_empty_strings() {
    let args = EnhancePromptArgs {
        project_root_path: Some("".to_string()),
        prompt: Some("".to_string()),
        conversation_history: Some("".to_string()),
    };

    assert_eq!(args.project_root_path, Some("".to_string()));
    assert_eq!(args.prompt, Some("".to_string()));
    assert_eq!(args.conversation_history, Some("".to_string()));
}

#[test]
fn test_enhance_prompt_args_whitespace_only() {
    let args = EnhancePromptArgs {
        project_root_path: Some("   ".to_string()),
        prompt: Some("\t\n".to_string()),
        conversation_history: Some("  \n  ".to_string()),
    };

    assert!(args.prompt.is_some());
}

#[test]
fn test_enhance_prompt_args_null_values_in_json() {
    let json = r#"{
        "project_root_path": null,
        "prompt": "test",
        "conversation_history": null
    }"#;

    let args: EnhancePromptArgs = serde_json::from_str(json).unwrap();
    assert!(args.project_root_path.is_none());
    assert_eq!(args.prompt, Some("test".to_string()));
    assert!(args.conversation_history.is_none());
}
