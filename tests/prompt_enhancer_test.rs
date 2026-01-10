//! Tests for prompt_enhancer module

use ace_tool::enhancer::prompt_enhancer::{get_enhancer_endpoint, ENV_ENHANCER_ENDPOINT};
use ace_tool::service::{
    extract_enhanced_prompt, get_third_party_config, is_chinese_text, parse_chat_history,
    parse_streaming_response, render_enhance_prompt, replace_tool_names, ChatMessage,
    EnhancerEndpoint, DEFAULT_CLAUDE_MODEL, DEFAULT_GEMINI_MODEL, DEFAULT_MODEL, DEFAULT_OPENAI_MODEL,
    ENV_ENHANCER_BASE_URL, ENV_ENHANCER_MODEL, ENV_ENHANCER_TOKEN, NODE_ID_NEW, NODE_ID_OLD,
};
use std::sync::Mutex;

/// Global mutex for tests that modify environment variables
/// All env-modifying tests must acquire this lock to prevent race conditions
static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
fn test_get_enhancer_endpoint_all_cases() {
    use std::sync::Mutex;
    static ENV_MUTEX: Mutex<()> = Mutex::new(());
    let _guard = ENV_MUTEX.lock().unwrap();

    let original_value = std::env::var(ENV_ENHANCER_ENDPOINT).ok();

    // Test default
    std::env::remove_var(ENV_ENHANCER_ENDPOINT);
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::New);

    // Test each endpoint type
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "old");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Old);

    std::env::set_var(ENV_ENHANCER_ENDPOINT, "new");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::New);

    std::env::set_var(ENV_ENHANCER_ENDPOINT, "claude");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Claude);

    std::env::set_var(ENV_ENHANCER_ENDPOINT, "openai");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::OpenAI);

    std::env::set_var(ENV_ENHANCER_ENDPOINT, "gemini");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Gemini);

    // Edge cases
    // Case insensitive
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "OLD");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Old);
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "Old");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Old);

    // Whitespace
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "  old  ");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::Old);

    // Invalid value -> Default (New)
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "invalid");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::New);

    // Empty string -> Default (New)
    std::env::set_var(ENV_ENHANCER_ENDPOINT, "");
    assert_eq!(get_enhancer_endpoint(), EnhancerEndpoint::New);

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

// ========================================================================
// EnhancerEndpoint Tests
// ========================================================================

#[test]
fn test_enhancer_endpoint_from_env_str() {
    assert_eq!(EnhancerEndpoint::from_env_str("old"), EnhancerEndpoint::Old);
    assert_eq!(EnhancerEndpoint::from_env_str("OLD"), EnhancerEndpoint::Old);
    assert_eq!(EnhancerEndpoint::from_env_str("Old"), EnhancerEndpoint::Old);
    assert_eq!(EnhancerEndpoint::from_env_str("new"), EnhancerEndpoint::New);
    assert_eq!(EnhancerEndpoint::from_env_str("NEW"), EnhancerEndpoint::New);
    assert_eq!(
        EnhancerEndpoint::from_env_str("claude"),
        EnhancerEndpoint::Claude
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("CLAUDE"),
        EnhancerEndpoint::Claude
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("Claude"),
        EnhancerEndpoint::Claude
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("openai"),
        EnhancerEndpoint::OpenAI
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("OPENAI"),
        EnhancerEndpoint::OpenAI
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("OpenAI"),
        EnhancerEndpoint::OpenAI
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("gemini"),
        EnhancerEndpoint::Gemini
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("GEMINI"),
        EnhancerEndpoint::Gemini
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("Gemini"),
        EnhancerEndpoint::Gemini
    );
}

#[test]
fn test_enhancer_endpoint_from_env_str_with_whitespace() {
    assert_eq!(
        EnhancerEndpoint::from_env_str("  claude  "),
        EnhancerEndpoint::Claude
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str("\topenai\n"),
        EnhancerEndpoint::OpenAI
    );
    assert_eq!(
        EnhancerEndpoint::from_env_str(" gemini "),
        EnhancerEndpoint::Gemini
    );
}

#[test]
fn test_enhancer_endpoint_from_env_str_unknown() {
    assert_eq!(
        EnhancerEndpoint::from_env_str("unknown"),
        EnhancerEndpoint::New
    );
    assert_eq!(EnhancerEndpoint::from_env_str(""), EnhancerEndpoint::New);
    assert_eq!(
        EnhancerEndpoint::from_env_str("invalid"),
        EnhancerEndpoint::New
    );
}

#[test]
fn test_enhancer_endpoint_is_third_party() {
    assert!(!EnhancerEndpoint::New.is_third_party());
    assert!(!EnhancerEndpoint::Old.is_third_party());
    assert!(EnhancerEndpoint::Claude.is_third_party());
    assert!(EnhancerEndpoint::OpenAI.is_third_party());
    assert!(EnhancerEndpoint::Gemini.is_third_party());
}

// ========================================================================
// ThirdPartyConfig Tests
// ========================================================================

#[test]
fn test_get_third_party_config_missing_base_url() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let orig_url = std::env::var(ENV_ENHANCER_BASE_URL).ok();
    let orig_token = std::env::var(ENV_ENHANCER_TOKEN).ok();

    std::env::remove_var(ENV_ENHANCER_BASE_URL);
    std::env::set_var(ENV_ENHANCER_TOKEN, "test-token");

    let result = get_third_party_config(EnhancerEndpoint::Claude);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("PROMPT_ENHANCER_BASE_URL"));

    // Restore
    match orig_url {
        Some(v) => std::env::set_var(ENV_ENHANCER_BASE_URL, v),
        None => std::env::remove_var(ENV_ENHANCER_BASE_URL),
    }
    match orig_token {
        Some(v) => std::env::set_var(ENV_ENHANCER_TOKEN, v),
        None => std::env::remove_var(ENV_ENHANCER_TOKEN),
    }
}

#[test]
fn test_get_third_party_config_missing_token() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let orig_url = std::env::var(ENV_ENHANCER_BASE_URL).ok();
    let orig_token = std::env::var(ENV_ENHANCER_TOKEN).ok();

    std::env::set_var(ENV_ENHANCER_BASE_URL, "https://api.example.com");
    std::env::remove_var(ENV_ENHANCER_TOKEN);

    let result = get_third_party_config(EnhancerEndpoint::Claude);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("PROMPT_ENHANCER_TOKEN"));

    // Restore
    match orig_url {
        Some(v) => std::env::set_var(ENV_ENHANCER_BASE_URL, v),
        None => std::env::remove_var(ENV_ENHANCER_BASE_URL),
    }
    match orig_token {
        Some(v) => std::env::set_var(ENV_ENHANCER_TOKEN, v),
        None => std::env::remove_var(ENV_ENHANCER_TOKEN),
    }
}

#[test]
fn test_get_third_party_config_default_models() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let orig_url = std::env::var(ENV_ENHANCER_BASE_URL).ok();
    let orig_token = std::env::var(ENV_ENHANCER_TOKEN).ok();
    let orig_model = std::env::var(ENV_ENHANCER_MODEL).ok();

    std::env::set_var(ENV_ENHANCER_BASE_URL, "https://api.example.com/");
    std::env::set_var(ENV_ENHANCER_TOKEN, "test-token");
    std::env::remove_var(ENV_ENHANCER_MODEL);

    // Test Claude default model
    let config = get_third_party_config(EnhancerEndpoint::Claude).unwrap();
    assert_eq!(config.model, DEFAULT_CLAUDE_MODEL);
    assert_eq!(config.base_url, "https://api.example.com"); // trailing slash removed
    assert_eq!(config.token, "test-token");

    // Test OpenAI default model
    let config = get_third_party_config(EnhancerEndpoint::OpenAI).unwrap();
    assert_eq!(config.model, DEFAULT_OPENAI_MODEL);

    // Test Gemini default model
    let config = get_third_party_config(EnhancerEndpoint::Gemini).unwrap();
    assert_eq!(config.model, DEFAULT_GEMINI_MODEL);

    // Restore
    match orig_url {
        Some(v) => std::env::set_var(ENV_ENHANCER_BASE_URL, v),
        None => std::env::remove_var(ENV_ENHANCER_BASE_URL),
    }
    match orig_token {
        Some(v) => std::env::set_var(ENV_ENHANCER_TOKEN, v),
        None => std::env::remove_var(ENV_ENHANCER_TOKEN),
    }
    match orig_model {
        Some(v) => std::env::set_var(ENV_ENHANCER_MODEL, v),
        None => std::env::remove_var(ENV_ENHANCER_MODEL),
    }
}

#[test]
fn test_get_third_party_config_custom_model() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let orig_url = std::env::var(ENV_ENHANCER_BASE_URL).ok();
    let orig_token = std::env::var(ENV_ENHANCER_TOKEN).ok();
    let orig_model = std::env::var(ENV_ENHANCER_MODEL).ok();

    std::env::set_var(ENV_ENHANCER_BASE_URL, "https://api.custom.com");
    std::env::set_var(ENV_ENHANCER_TOKEN, "custom-token");
    std::env::set_var(ENV_ENHANCER_MODEL, "custom-model-v1");

    let config = get_third_party_config(EnhancerEndpoint::Claude).unwrap();
    assert_eq!(config.model, "custom-model-v1");

    // Restore
    match orig_url {
        Some(v) => std::env::set_var(ENV_ENHANCER_BASE_URL, v),
        None => std::env::remove_var(ENV_ENHANCER_BASE_URL),
    }
    match orig_token {
        Some(v) => std::env::set_var(ENV_ENHANCER_TOKEN, v),
        None => std::env::remove_var(ENV_ENHANCER_TOKEN),
    }
    match orig_model {
        Some(v) => std::env::set_var(ENV_ENHANCER_MODEL, v),
        None => std::env::remove_var(ENV_ENHANCER_MODEL),
    }
}

// ========================================================================
// Environment Variable Constants Tests
// ========================================================================

#[test]
fn test_env_var_constants() {
    assert_eq!(ENV_ENHANCER_ENDPOINT, "ACE_ENHANCER_ENDPOINT");
    assert_eq!(ENV_ENHANCER_BASE_URL, "PROMPT_ENHANCER_BASE_URL");
    assert_eq!(ENV_ENHANCER_TOKEN, "PROMPT_ENHANCER_TOKEN");
    assert_eq!(ENV_ENHANCER_MODEL, "PROMPT_ENHANCER_MODEL");
}

#[test]
fn test_default_model_constants() {
    assert_eq!(DEFAULT_CLAUDE_MODEL, "claude-sonnet-4-20250514");
    assert_eq!(DEFAULT_OPENAI_MODEL, "gpt-4o");
    assert_eq!(DEFAULT_GEMINI_MODEL, "gemini-2.0-flash-exp");
}
