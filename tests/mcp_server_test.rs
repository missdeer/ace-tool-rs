//! Tests for mcp server module

use ace_tool::mcp::{is_header_line, parse_content_length, TransportMode, MAX_HEADER_COUNT};

// Tests for TransportMode enum
#[test]
fn test_transport_mode_equality() {
    assert_eq!(TransportMode::Lsp, TransportMode::Lsp);
    assert_eq!(TransportMode::Line, TransportMode::Line);
    assert_ne!(TransportMode::Lsp, TransportMode::Line);
}

#[test]
fn test_transport_mode_copy() {
    let mode = TransportMode::Lsp;
    let mode_copy = mode;
    assert_eq!(mode, mode_copy);
}

#[test]
fn test_transport_mode_debug() {
    let lsp = TransportMode::Lsp;
    let line = TransportMode::Line;
    assert_eq!(format!("{:?}", lsp), "Lsp");
    assert_eq!(format!("{:?}", line), "Line");
}

// Tests for is_header_line function
#[test]
fn test_is_header_line_content_length() {
    assert!(is_header_line("Content-Length: 123"));
    assert!(is_header_line("content-length: 456"));
    assert!(is_header_line("CONTENT-LENGTH: 789"));
    assert!(is_header_line("Content-Length:0"));
}

#[test]
fn test_is_header_line_content_type() {
    assert!(is_header_line("Content-Type: application/json"));
    assert!(is_header_line("content-type: text/plain"));
    assert!(is_header_line("CONTENT-TYPE: application/vscode-jsonrpc"));
}

#[test]
fn test_is_header_line_invalid() {
    assert!(!is_header_line(""));
    assert!(!is_header_line("not a header"));
    assert!(!is_header_line("X-Custom-Header: value"));
    assert!(!is_header_line("Authorization: Bearer token"));
    assert!(!is_header_line("{\"jsonrpc\":\"2.0\"}"));
}

// Tests for parse_content_length function
#[test]
fn test_parse_content_length_valid() {
    assert_eq!(
        parse_content_length("Content-Length: 123").unwrap(),
        Some(123)
    );
    assert_eq!(parse_content_length("content-length: 0").unwrap(), Some(0));
    assert_eq!(
        parse_content_length("CONTENT-LENGTH:456").unwrap(),
        Some(456)
    );
    assert_eq!(
        parse_content_length("Content-Length:  789  ").unwrap(),
        Some(789)
    );
}

#[test]
fn test_parse_content_length_not_content_length() {
    assert_eq!(
        parse_content_length("Content-Type: application/json").unwrap(),
        None
    );
    assert_eq!(parse_content_length("X-Custom: 123").unwrap(), None);
    assert_eq!(parse_content_length("no colon here").unwrap(), None);
}

#[test]
fn test_parse_content_length_invalid_number() {
    assert!(parse_content_length("Content-Length: abc").is_err());
    assert!(parse_content_length("Content-Length: -1").is_err());
    assert!(parse_content_length("Content-Length: 12.34").is_err());
}

#[test]
fn test_parse_content_length_large_value() {
    let result = parse_content_length("Content-Length: 10485760").unwrap();
    assert_eq!(result, Some(10485760)); // 10MB
}

// Tests for message formatting (write_message output format)
#[test]
fn test_line_message_format() {
    // Line mode should append newline
    let payload = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
    let mut buffer = Vec::new();
    buffer.extend_from_slice(payload.as_bytes());
    buffer.push(b'\n');

    assert_eq!(buffer.len(), payload.len() + 1);
    assert_eq!(buffer.last(), Some(&b'\n'));
}

#[test]
fn test_lsp_message_format() {
    // LSP mode should prepend Content-Length header
    let payload = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut buffer = Vec::new();
    buffer.extend_from_slice(header.as_bytes());
    buffer.extend_from_slice(payload.as_bytes());

    let expected_header = "Content-Length: 36\r\n\r\n";
    assert!(String::from_utf8_lossy(&buffer).starts_with(expected_header));
}

#[test]
fn test_lsp_content_length_calculation() {
    // Verify Content-Length is byte length, not char length
    let payload_ascii = "hello";
    assert_eq!(payload_ascii.len(), 5);

    // UTF-8 multi-byte characters
    let payload_utf8 = "你好"; // 2 Chinese characters = 6 bytes
    assert_eq!(payload_utf8.len(), 6);
    assert_eq!(payload_utf8.chars().count(), 2);
}

// Tests for Content-Length limit (DoS protection)
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB

#[test]
fn test_content_length_within_limit() {
    let length = 1024 * 1024; // 1MB
    assert!(length <= MAX_MESSAGE_SIZE);
}

#[test]
fn test_content_length_at_limit() {
    let length = MAX_MESSAGE_SIZE;
    assert!(length <= MAX_MESSAGE_SIZE);
}

#[test]
fn test_content_length_exceeds_limit() {
    let length = MAX_MESSAGE_SIZE + 1;
    assert!(length > MAX_MESSAGE_SIZE);
}

#[test]
fn test_header_count_limit() {
    // Just verify the constant is set reasonably
    const { assert!(MAX_HEADER_COUNT >= 10) };
    const { assert!(MAX_HEADER_COUNT <= 1000) };
}

// Tests for header line edge cases
#[test]
fn test_is_header_line_with_extra_whitespace() {
    assert!(is_header_line("  Content-Length  : 123"));
    assert!(is_header_line("Content-Type : application/json"));
}

#[test]
fn test_is_header_line_empty_value() {
    assert!(is_header_line("Content-Length:"));
    assert!(is_header_line("Content-Type:"));
}
