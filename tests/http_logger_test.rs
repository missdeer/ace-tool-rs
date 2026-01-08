//! Tests for http_logger module

use ace_tool::http_logger::{is_sensitive_header, mask_token, truncate_utf8_safe};

#[test]
fn test_truncate_utf8_safe_ascii() {
    let s = "Hello, World!";
    assert_eq!(truncate_utf8_safe(s, 100), s);
    assert!(truncate_utf8_safe(s, 5).starts_with("Hello"));
}

#[test]
fn test_truncate_utf8_safe_unicode() {
    let s = "你好世界Hello";
    // Each Chinese char is 3 bytes, so 12 bytes for 4 chars + 5 bytes for Hello = 17 bytes
    let truncated = truncate_utf8_safe(s, 10);
    // Should not panic and should end at char boundary
    assert!(truncated.contains("..."));
    assert!(truncated.contains("[truncated"));
}

#[test]
fn test_mask_token_bearer() {
    assert_eq!(mask_token("Bearer abcdefghijklmnop"), "Bearer abcd...mnop");
    assert_eq!(mask_token("Bearer short"), "Bearer ****");
}

#[test]
fn test_mask_token_generic() {
    assert_eq!(mask_token("abcdefghijklmnop"), "abcd...mnop");
    assert_eq!(mask_token("short"), "****");
}

#[test]
fn test_is_sensitive_header() {
    assert!(is_sensitive_header("Authorization"));
    assert!(is_sensitive_header("authorization"));
    assert!(is_sensitive_header("Set-Cookie"));
    assert!(is_sensitive_header("set-cookie"));
    assert!(is_sensitive_header("Cookie"));
    assert!(!is_sensitive_header("Content-Type"));
}
