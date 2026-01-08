//! Tests for prompt_enhancer module

use ace_tool::enhancer::prompt_enhancer::{
    extract_enhanced_prompt, is_chinese_text, parse_chat_history, parse_streaming_response,
    render_enhance_prompt, replace_tool_names, ChatMessage, DEFAULT_MODEL, ENV_ENHANCER_ENDPOINT,
    NODE_ID_NEW, NODE_ID_OLD,
};

// ========================================================================
// is_chinese_text Tests
// ========================================================================

#[test]
fn test_is_chinese_text() {
    assert!(is_chinese_text("你好世界")); // 4 Chinese chars >= 3
    assert!(is_chinese_text("Hello 中文好")); // 3 Chinese chars >= 3
    assert!(!is_chinese_text("Hello World"));
    assert!(!is_chinese_text("123"));
}

#[test]
fn test_is_chinese_text_pure_chinese() {
    assert!(is_chinese_text("这是纯中文文本")); // Many Chinese chars
    assert!(is_chinese_text("中")); // 1 Chinese char = 100% of content
}

#[test]
fn test_is_chinese_text_mixed() {
    assert!(is_chinese_text("Hello中文World")); // 2 Chinese chars but > 10% of non-ws
    assert!(is_chinese_text("123中456")); // 1 Chinese char but > 10% (1/6 = 16%)
    assert!(is_chinese_text("test 测试 test")); // 2 Chinese chars, 2/12 = 16%
}

#[test]
fn test_is_chinese_text_threshold() {
    // Test the 10% threshold
    assert!(!is_chinese_text("This is a very long English text with 中")); // 1 char, < 10%
    assert!(is_chinese_text("中文测试")); // 4 chars >= 3
    assert!(is_chinese_text("abc中文")); // 2 Chinese chars, 2/5 = 40%
}

#[test]
fn test_is_chinese_text_empty() {
    assert!(!is_chinese_text(""));
}

#[test]
fn test_is_chinese_text_whitespace_only() {
    assert!(!is_chinese_text("   "));
    assert!(!is_chinese_text("\t\n"));
}

#[test]
fn test_is_chinese_text_special_chars() {
    assert!(!is_chinese_text("@#$%^&*()"));
    assert!(!is_chinese_text(".,;:!?"));
}

#[test]
fn test_is_chinese_text_japanese() {
    // Japanese hiragana/katakana should not match Chinese regex
    assert!(!is_chinese_text("こんにちは")); // Hiragana
    assert!(!is_chinese_text("カタカナ")); // Katakana
}

#[test]
fn test_is_chinese_text_korean() {
    assert!(!is_chinese_text("안녕하세요")); // Korean
}

#[test]
fn test_is_chinese_text_numbers_and_punctuation() {
    assert!(!is_chinese_text("12345"));
    assert!(!is_chinese_text("..."));
    assert!(is_chinese_text("数字123")); // 2 Chinese chars, 2/5 = 40%
}

#[test]
fn test_is_chinese_text_chinese_punctuation() {
    // Chinese punctuation alone doesn't make it Chinese
    assert!(!is_chinese_text("。，！？"));
    // But with Chinese characters, it should
    assert!(is_chinese_text("你好！")); // 2 Chinese chars, 2/3 = 66%
}

// ========================================================================
// extract_enhanced_prompt Tests
// ========================================================================

#[test]
fn test_extract_enhanced_prompt_basic() {
    let text = "<augment-enhanced-prompt>Enhanced content here</augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("Enhanced content here".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_with_surrounding_text() {
    let text = r#"### BEGIN RESPONSE ###
Here is an enhanced version:
<augment-enhanced-prompt>The enhanced prompt</augment-enhanced-prompt>

### END RESPONSE ###"#;
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("The enhanced prompt".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_multiline() {
    let text = r#"<augment-enhanced-prompt>
Line 1
Line 2
Line 3
</augment-enhanced-prompt>"#;
    let result = extract_enhanced_prompt(text);
    assert!(result.is_some());
    let content = result.unwrap();
    assert!(content.contains("Line 1"));
    assert!(content.contains("Line 2"));
    assert!(content.contains("Line 3"));
}

#[test]
fn test_extract_enhanced_prompt_no_tag() {
    let text = "Just some plain text without tags";
    let result = extract_enhanced_prompt(text);
    assert!(result.is_none());
}

#[test]
fn test_extract_enhanced_prompt_empty_tag() {
    let text = "<augment-enhanced-prompt></augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert!(result.is_none());
}

#[test]
fn test_extract_enhanced_prompt_whitespace_trimmed() {
    let text = "<augment-enhanced-prompt>  \n  content  \n  </augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("content".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_chinese() {
    let text = "<augment-enhanced-prompt>添加用户登录功能</augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("添加用户登录功能".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_special_chars() {
    let text = "<augment-enhanced-prompt>Use `code` and \"quotes\"</augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("Use `code` and \"quotes\"".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_tag_with_whitespace() {
    let text = "<augment-enhanced-prompt >  content </augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("content".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_tag_with_attributes() {
    let text = "<augment-enhanced-prompt id=\"test\">content here</augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("content here".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_closing_tag_whitespace() {
    let text = "<augment-enhanced-prompt>content</augment-enhanced-prompt >";
    let result = extract_enhanced_prompt(text);
    assert_eq!(result, Some("content".to_string()));
}

#[test]
fn test_extract_enhanced_prompt_whitespace_only_content() {
    let text = "<augment-enhanced-prompt>   \n\t  </augment-enhanced-prompt>";
    let result = extract_enhanced_prompt(text);
    assert!(result.is_none());
}

// ========================================================================
// render_enhance_prompt Tests
// ========================================================================

#[test]
fn test_render_enhance_prompt_preserves_placeholders_in_input() {
    // User content containing placeholder-like text should remain intact
    let prompt = "Use {conversation_history} and {original_prompt} literally";
    let result = render_enhance_prompt(prompt).unwrap();

    // The inserted content with placeholder-like text remains intact
    assert!(result.contains(prompt));
}

#[test]
fn test_render_enhance_prompt_basic() {
    let prompt = "Add feature";
    let result = render_enhance_prompt(prompt).unwrap();

    assert!(result.contains("Add feature"));
    assert!(result.contains("NO TOOLS ALLOWED"));
    assert!(result.contains("triple backticks"));
}

// ========================================================================
// replace_tool_names Tests
// ========================================================================

#[test]
fn test_replace_tool_names() {
    let text = "Use codebase-retrieval to search";
    let result = replace_tool_names(text);
    assert_eq!(result, "Use search_context to search");

    let text2 = "Use codebase_retrieval to search";
    let result2 = replace_tool_names(text2);
    assert_eq!(result2, "Use search_context to search");
}

#[test]
fn test_replace_tool_names_multiple_occurrences() {
    let text = "First codebase-retrieval then codebase-retrieval again";
    let result = replace_tool_names(text);
    assert_eq!(result, "First search_context then search_context again");
}

#[test]
fn test_replace_tool_names_mixed() {
    let text = "Use codebase-retrieval and codebase_retrieval";
    let result = replace_tool_names(text);
    assert_eq!(result, "Use search_context and search_context");
}

#[test]
fn test_replace_tool_names_no_match() {
    let text = "Use search_context directly";
    let result = replace_tool_names(text);
    assert_eq!(result, "Use search_context directly");
}

#[test]
fn test_replace_tool_names_empty() {
    let result = replace_tool_names("");
    assert_eq!(result, "");
}

#[test]
fn test_replace_tool_names_preserves_case() {
    let text = "CODEBASE-RETRIEVAL"; // Won't match (case sensitive)
    let result = replace_tool_names(text);
    assert_eq!(result, "CODEBASE-RETRIEVAL");
}

#[test]
fn test_replace_tool_names_in_code_block() {
    let text = "```\ncodebase-retrieval query\n```";
    let result = replace_tool_names(text);
    assert!(result.contains("search_context"));
}

#[test]
fn test_replace_tool_names_in_json() {
    let text = r#"{"tool": "codebase-retrieval", "args": {}}"#;
    let result = replace_tool_names(text);
    assert!(result.contains("search_context"));
}

// ========================================================================
// parse_chat_history Tests
// ========================================================================

#[test]
fn test_parse_chat_history() {
    let history = "User: Hello\nAssistant: Hi there\n用户: 你好\n助手: 你好！";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 4);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content, "Hello");
    assert_eq!(result[1].role, "assistant");
    assert_eq!(result[1].content, "Hi there");
    assert_eq!(result[2].role, "user");
    assert_eq!(result[2].content, "你好");
    assert_eq!(result[3].role, "assistant");
    assert_eq!(result[3].content, "你好！");
}

#[test]
fn test_parse_chat_history_empty() {
    let result = parse_chat_history("");
    assert!(result.is_empty());
}

#[test]
fn test_parse_chat_history_whitespace_only() {
    let result = parse_chat_history("   \n\t\n   ");
    assert!(result.is_empty());
}

#[test]
fn test_parse_chat_history_user_only() {
    let history = "User: Hello world";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content, "Hello world");
}

#[test]
fn test_parse_chat_history_assistant_only() {
    let history = "Assistant: I can help with that";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    assert_eq!(result[0].content, "I can help with that");
}

#[test]
fn test_parse_chat_history_ai_prefix() {
    let history = "AI: This is an AI response";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    assert_eq!(result[0].content, "This is an AI response");
}

#[test]
fn test_parse_chat_history_chinese_prefixes() {
    let history = "用户: 你好\n助手: 你好！有什么可以帮助你的？";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content, "你好");
    assert_eq!(result[1].role, "assistant");
    assert!(result[1].content.contains("帮助"));
}

#[test]
fn test_parse_chat_history_with_extra_whitespace() {
    let history = "User:   Hello with spaces   ";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content, "Hello with spaces");
}

#[test]
fn test_parse_chat_history_ignores_unknown_prefixes() {
    let history = "System: Internal message\nUser: Hello";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content, "Hello");
}

#[test]
fn test_parse_chat_history_with_colons_in_content() {
    let history = "User: Time is 10:30:00";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content, "Time is 10:30:00");
}

#[test]
fn test_parse_chat_history_multiline_message() {
    let history = "User: Line 1\nLine 2\nAssistant: Response";
    let result = parse_chat_history(history);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content, "Line 1\nLine 2");
    assert_eq!(result[1].role, "assistant");
    assert_eq!(result[1].content, "Response");
}

#[test]
fn test_parse_chat_history_long_conversation() {
    let history = (0..20)
        .map(|i| format!("User: Message {}\nAssistant: Response {}", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    let result = parse_chat_history(&history);
    assert_eq!(result.len(), 40);
}

// ========================================================================
// ChatMessage Tests
// ========================================================================

#[test]
fn test_chat_message_clone() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello".to_string(),
    };

    let cloned = msg.clone();
    assert_eq!(cloned.role, msg.role);
    assert_eq!(cloned.content, msg.content);
}

#[test]
fn test_chat_message_serialization() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("\"content\":\"Hello\""));
}

#[test]
fn test_chat_message_deserialization() {
    let json = r#"{"role":"assistant","content":"Hi there"}"#;
    let msg: ChatMessage = serde_json::from_str(json).unwrap();

    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "Hi there");
}

// ========================================================================
// Integration-like Tests
// ========================================================================

#[test]
fn test_language_detection_for_enhancement() {
    // Simulate language detection for API guideline
    let chinese_prompt = "添加一个登录功能";
    let english_prompt = "Add a login feature";

    assert!(is_chinese_text(chinese_prompt));
    assert!(!is_chinese_text(english_prompt));
}

#[test]
fn test_full_workflow_simulation() {
    // Simulate a typical enhancement workflow
    let original_prompt = "新加一个登录页面";
    let conversation = "User: 我在开发一个web应用\n助手: 好的，我可以帮助你";

    // Check language detection
    assert!(is_chinese_text(original_prompt));

    // Parse conversation
    let history = parse_chat_history(conversation);
    assert_eq!(history.len(), 2);

    // Simulate enhanced output with tool name replacement
    let enhanced = "请使用 codebase-retrieval 工具来搜索";
    let replaced = replace_tool_names(enhanced);
    assert!(replaced.contains("search_context"));
}

// ========================================================================
// Endpoint Selection Tests
// Note: Environment variable tests are combined into one to avoid race conditions
// ========================================================================

#[test]
fn test_use_new_endpoint_all_cases() {
    use ace_tool::enhancer::prompt_enhancer::use_new_endpoint;

    // Use a static mutex to prevent parallel execution issues
    use std::sync::Mutex;
    static ENV_MUTEX: Mutex<()> = Mutex::new(());
    let _guard = ENV_MUTEX.lock().unwrap();

    // Save original value to restore later
    let original_value = std::env::var(ENV_ENHANCER_ENDPOINT).ok();

    // Test 1: Default should be new endpoint
    std::env::remove_var(ENV_ENHANCER_ENDPOINT);
    assert!(use_new_endpoint(), "Default should use new endpoint");

    // Test 2: Explicit "new" should use new endpoint
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "new");
    assert!(use_new_endpoint(), "\"new\" should use new endpoint");

    // Test 3: Case insensitive - "NEW"
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "NEW");
    assert!(use_new_endpoint(), "\"NEW\" should use new endpoint");

    // Test 4: Case insensitive - "New"
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "New");
    assert!(use_new_endpoint(), "\"New\" should use new endpoint");

    // Test 5: Explicit "old" should use old endpoint
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "old");
    assert!(!use_new_endpoint(), "\"old\" should use old endpoint");

    // Test 6: Explicit "OLD" should use old endpoint (case insensitive)
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "OLD");
    assert!(!use_new_endpoint(), "\"OLD\" should use old endpoint");

    // Test 7: Invalid value should default to new endpoint
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "invalid");
    assert!(use_new_endpoint(), "Invalid value should use new endpoint");

    // Test 8: Whitespace should be trimmed
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "  new  ");
    assert!(use_new_endpoint(), "Whitespace should be trimmed");

    // Test 9: Empty string should use new endpoint
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "");
    assert!(use_new_endpoint(), "Empty string should use new endpoint");

    // Test 10: Whitespace only should use new endpoint
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "   ");
    assert!(
        use_new_endpoint(),
        "Whitespace only should use new endpoint"
    );

    // Test 11: Newlines should be trimmed
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "\nnew\n");
    assert!(use_new_endpoint(), "Newlines should be trimmed");

    // Test 12: Mixed case variations
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "nEw");
    assert!(use_new_endpoint(), "Mixed case nEw should work");

    std::env::set_var(ENV_ENHANCER_ENDPOINT, "nEW");
    assert!(use_new_endpoint(), "Mixed case nEW should work");

    // Test 13: Explicit "old" with whitespace
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "  old  ");
    assert!(!use_new_endpoint(), "\"  old  \" should use old endpoint");

    // Test 14: Tabs in value
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "\told\t");
    assert!(!use_new_endpoint(), "Tabs around old should be trimmed");

    // Test 15: Mixed whitespace around old
    std::env::set_var(ENV_ENHANCER_ENDPOINT, " \t\nold\n\t ");
    assert!(
        !use_new_endpoint(),
        "Mixed whitespace around old should be trimmed"
    );

    // Restore original value
    match original_value {
        Some(v) => std::env::set_var(ENV_ENHANCER_ENDPOINT, v),
        None => std::env::remove_var(ENV_ENHANCER_ENDPOINT),
    }
}

// ========================================================================
// NODE_ID Constants Tests
// ========================================================================

#[test]
fn test_node_id_constants_values() {
    assert_eq!(NODE_ID_NEW, 0);
    assert_eq!(NODE_ID_OLD, 1);
}

#[test]
fn test_node_id_constants_are_different() {
    assert_ne!(NODE_ID_NEW, NODE_ID_OLD);
}

// ========================================================================
// Default Model Tests
// ========================================================================

#[test]
fn test_default_model_constant() {
    assert_eq!(DEFAULT_MODEL, "claude-sonnet-4-5");
}

// ========================================================================
// Environment Variable Edge Cases Tests
// ========================================================================

#[test]
fn test_env_enhancer_endpoint_constant() {
    assert_eq!(ENV_ENHANCER_ENDPOINT, "ACE_ENHANCER_ENDPOINT");
}

// ========================================================================
// parse_streaming_response Tests
// ========================================================================

#[test]
fn test_parse_streaming_response_single_line() {
    let body = r#"{"text":"Hello World"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn test_parse_streaming_response_multiple_lines() {
    let body = r#"{"text":"Hello "}
{"text":"World"}
{"text":"!"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Hello World!");
}

#[test]
fn test_parse_streaming_response_with_empty_lines() {
    let body = r#"{"text":"Hello "}

{"text":"World"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn test_parse_streaming_response_with_whitespace() {
    let body = r#"  {"text":"Hello "}
   {"text":"World"}   "#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn test_parse_streaming_response_with_null_text() {
    let body = r#"{"text":"Hello "}
{"text":null}
{"text":"World"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn test_parse_streaming_response_fallback_to_single_json() {
    // If no lines can be parsed as streaming, fall back to single JSON parse
    let body = r#"{"text":"Single JSON response"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Single JSON response");
}

#[test]
fn test_parse_streaming_response_with_xml_tag() {
    let body = r#"{"text":"<augment-enhanced-prompt>"}
{"text":"Enhanced prompt content"}
{"text":"</augment-enhanced-prompt>"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert!(result.contains("<augment-enhanced-prompt>"));
    assert!(result.contains("Enhanced prompt content"));
    assert!(result.contains("</augment-enhanced-prompt>"));
}

#[test]
fn test_parse_streaming_response_chinese() {
    let body = r#"{"text":"你好"}
{"text":"世界"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "你好世界");
}

#[test]
fn test_parse_streaming_response_mixed_valid_invalid() {
    let body = r#"{"text":"Valid "}
not a json line
{"text":"content"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "Valid content");
}

#[test]
fn test_parse_streaming_response_empty_text() {
    let body = r#"{"text":""}
{"text":"content"}"#;
    let result = parse_streaming_response(body).unwrap();
    assert_eq!(result, "content");
}
