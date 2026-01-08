//! Tests for enhancer templates module

use ace_tool::enhancer::templates::{ENHANCER_UI_HTML, ENHANCE_PROMPT_TEMPLATE};

// ========================================================================
// ENHANCE_PROMPT_TEMPLATE Tests
// ========================================================================

#[test]
fn test_enhance_prompt_template_not_empty() {
    assert!(!ENHANCE_PROMPT_TEMPLATE.is_empty());
}

#[test]
fn test_enhance_prompt_template_has_original_prompt_placeholder() {
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("{original_prompt}"));
}

#[test]
fn test_enhance_prompt_template_has_no_tools_warning() {
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("NO TOOLS ALLOWED"));
}

#[test]
fn test_enhance_prompt_template_has_response_format() {
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("### BEGIN RESPONSE ###"));
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("### END RESPONSE ###"));
}

#[test]
fn test_enhance_prompt_template_has_augment_tag() {
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("<augment-enhanced-prompt>"));
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("</augment-enhanced-prompt>"));
}

#[test]
fn test_enhance_prompt_template_has_code_block_instruction() {
    // Must match augment.mjs: mentions triple backticks for code samples
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("triple backticks"));
    assert!(ENHANCE_PROMPT_TEMPLATE.contains("code sample"));
}

#[test]
fn test_enhance_prompt_template_replace_works() {
    let result = ENHANCE_PROMPT_TEMPLATE.replace("{original_prompt}", "Add login feature");

    assert!(result.contains("Add login feature"));
    assert!(!result.contains("{original_prompt}"));
}

#[test]
fn test_enhance_prompt_template_empty_value() {
    let result = ENHANCE_PROMPT_TEMPLATE.replace("{original_prompt}", "");

    assert!(!result.contains("{original_prompt}"));
}

#[test]
fn test_enhance_prompt_template_unicode_value() {
    let result = ENHANCE_PROMPT_TEMPLATE.replace("{original_prompt}", "添加登录功能");

    assert!(result.contains("添加登录功能"));
}

// ========================================================================
// HTML Template Content Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_not_empty() {
    assert!(!ENHANCER_UI_HTML.is_empty());
}

#[test]
fn test_enhancer_ui_html_is_valid_html() {
    assert!(ENHANCER_UI_HTML.starts_with("<!DOCTYPE html>"));
    assert!(ENHANCER_UI_HTML.contains("<html"));
    assert!(ENHANCER_UI_HTML.contains("</html>"));
}

#[test]
fn test_enhancer_ui_html_has_head() {
    assert!(ENHANCER_UI_HTML.contains("<head>"));
    assert!(ENHANCER_UI_HTML.contains("</head>"));
}

#[test]
fn test_enhancer_ui_html_has_body() {
    assert!(ENHANCER_UI_HTML.contains("<body>"));
    assert!(ENHANCER_UI_HTML.contains("</body>"));
}

#[test]
fn test_enhancer_ui_html_has_title() {
    assert!(ENHANCER_UI_HTML.contains("<title>"));
    assert!(ENHANCER_UI_HTML.contains("Prompt Enhancer"));
    assert!(ENHANCER_UI_HTML.contains("ACE Tool"));
}

#[test]
fn test_enhancer_ui_html_has_charset() {
    assert!(ENHANCER_UI_HTML.contains("charset=\"UTF-8\""));
}

#[test]
fn test_enhancer_ui_html_has_viewport() {
    assert!(ENHANCER_UI_HTML.contains("viewport"));
}

// ========================================================================
// Button Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_send_button() {
    assert!(ENHANCER_UI_HTML.contains("sendPrompt"));
    assert!(ENHANCER_UI_HTML.contains("Send Enhanced") || ENHANCER_UI_HTML.contains("send-btn"));
}

#[test]
fn test_enhancer_ui_html_has_use_original_button() {
    assert!(ENHANCER_UI_HTML.contains("useOriginal"));
    assert!(
        ENHANCER_UI_HTML.contains("Use Original") || ENHANCER_UI_HTML.contains("__USE_ORIGINAL__")
    );
}

#[test]
fn test_enhancer_ui_html_has_re_enhance_button() {
    assert!(ENHANCER_UI_HTML.contains("reEnhance"));
    assert!(ENHANCER_UI_HTML.contains("Re-enhance") || ENHANCER_UI_HTML.contains("re-enhance-btn"));
}

#[test]
fn test_enhancer_ui_html_has_end_conversation_button() {
    assert!(ENHANCER_UI_HTML.contains("endConversation"));
    assert!(
        ENHANCER_UI_HTML.contains("End Chat") || ENHANCER_UI_HTML.contains("__END_CONVERSATION__")
    );
}

// ========================================================================
// API Endpoints Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_session_endpoint() {
    assert!(ENHANCER_UI_HTML.contains("/api/session"));
}

#[test]
fn test_enhancer_ui_html_has_submit_endpoint() {
    assert!(ENHANCER_UI_HTML.contains("/api/submit"));
}

#[test]
fn test_enhancer_ui_html_has_re_enhance_endpoint() {
    assert!(ENHANCER_UI_HTML.contains("/api/re-enhance"));
}

// ========================================================================
// JavaScript Function Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_script_tag() {
    assert!(ENHANCER_UI_HTML.contains("<script>"));
    assert!(ENHANCER_UI_HTML.contains("</script>"));
}

#[test]
fn test_enhancer_ui_html_has_countdown_function() {
    assert!(ENHANCER_UI_HTML.contains("updateCountdown"));
}

#[test]
fn test_enhancer_ui_html_has_char_count_function() {
    assert!(ENHANCER_UI_HTML.contains("updateCharCount"));
}

#[test]
fn test_enhancer_ui_html_has_status_function() {
    assert!(ENHANCER_UI_HTML.contains("showStatus"));
}

// ========================================================================
// Keyboard Shortcuts Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_keyboard_shortcuts() {
    assert!(ENHANCER_UI_HTML.contains("keydown"));
    assert!(ENHANCER_UI_HTML.contains("Ctrl") || ENHANCER_UI_HTML.contains("ctrlKey"));
    assert!(ENHANCER_UI_HTML.contains("Enter"));
    assert!(ENHANCER_UI_HTML.contains("Escape") || ENHANCER_UI_HTML.contains("Esc"));
}

// ========================================================================
// CSS Style Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_style_tag() {
    assert!(ENHANCER_UI_HTML.contains("<style>"));
    assert!(ENHANCER_UI_HTML.contains("</style>"));
}

#[test]
fn test_enhancer_ui_html_has_button_styles() {
    assert!(ENHANCER_UI_HTML.contains(".send-btn"));
    assert!(ENHANCER_UI_HTML.contains(".cancel-btn"));
    assert!(ENHANCER_UI_HTML.contains(".re-enhance-btn"));
}

#[test]
fn test_enhancer_ui_html_has_status_styles() {
    assert!(ENHANCER_UI_HTML.contains(".status"));
    assert!(ENHANCER_UI_HTML.contains(".success"));
    assert!(ENHANCER_UI_HTML.contains(".error"));
}

#[test]
fn test_enhancer_ui_html_has_countdown_styles() {
    assert!(ENHANCER_UI_HTML.contains(".countdown"));
    assert!(ENHANCER_UI_HTML.contains(".warning"));
    assert!(ENHANCER_UI_HTML.contains(".danger"));
}

#[test]
fn test_enhancer_ui_html_has_spinner_animation() {
    assert!(ENHANCER_UI_HTML.contains(".spinner"));
    assert!(ENHANCER_UI_HTML.contains("@keyframes"));
}

// ========================================================================
// Textarea Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_textarea() {
    assert!(ENHANCER_UI_HTML.contains("<textarea"));
    assert!(ENHANCER_UI_HTML.contains("promptText"));
}

#[test]
fn test_enhancer_ui_html_has_char_count_display() {
    assert!(ENHANCER_UI_HTML.contains("charCount"));
}

// ========================================================================
// Session ID Handling Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_reads_session_from_url() {
    assert!(ENHANCER_UI_HTML.contains("URLSearchParams"));
    assert!(ENHANCER_UI_HTML.contains("session"));
}

#[test]
fn test_enhancer_ui_html_sends_session_id() {
    assert!(ENHANCER_UI_HTML.contains("sessionId"));
}

// ========================================================================
// Responsive Design Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_media_queries() {
    assert!(ENHANCER_UI_HTML.contains("@media"));
}

// ========================================================================
// Accessibility Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_lang_attribute() {
    assert!(ENHANCER_UI_HTML.contains("lang="));
}

// ========================================================================
// Template Size Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_reasonable_size() {
    // Template should be substantial but not too large
    let size = ENHANCER_UI_HTML.len();
    assert!(size > 5000, "Template seems too small: {} bytes", size);
    assert!(size < 100000, "Template seems too large: {} bytes", size);
}

// ========================================================================
// Action Field Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_use_original_action() {
    assert!(ENHANCER_UI_HTML.contains("action: 'use_original'"));
}

#[test]
fn test_enhancer_ui_html_has_end_conversation_action() {
    assert!(ENHANCER_UI_HTML.contains("action: 'end_conversation'"));
}

#[test]
fn test_enhancer_ui_html_has_send_action() {
    assert!(ENHANCER_UI_HTML.contains("action: 'send'"));
}

// ========================================================================
// Fetch API Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_uses_fetch() {
    assert!(ENHANCER_UI_HTML.contains("fetch("));
}

#[test]
fn test_enhancer_ui_html_uses_json() {
    assert!(ENHANCER_UI_HTML.contains("application/json"));
    assert!(ENHANCER_UI_HTML.contains("JSON.stringify"));
}

// ========================================================================
// Timer/Countdown Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_has_interval() {
    assert!(ENHANCER_UI_HTML.contains("setInterval") || ENHANCER_UI_HTML.contains("interval"));
}

#[test]
fn test_enhancer_ui_html_has_timeout_handling() {
    assert!(ENHANCER_UI_HTML.contains("timeout") || ENHANCER_UI_HTML.contains("Timeout"));
}

// ========================================================================
// Window Close Tests
// ========================================================================

#[test]
fn test_enhancer_ui_html_can_close_window() {
    assert!(ENHANCER_UI_HTML.contains("window.close"));
}
