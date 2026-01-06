//! HTTP Request Logger
//!
//! Logs all HTTP requests to a file when enabled via environment variable.
//! Set `ACE_HTTP_LOG=1` or `ACE_HTTP_LOG=true` to enable.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use tracing::warn;

/// Environment variable to control HTTP logging
const ENV_HTTP_LOG: &str = "ACE_HTTP_LOG";

/// Log file name
const LOG_FILE_NAME: &str = "http_requests.log";

/// Maximum body size to log (10KB)
const MAX_BODY_SIZE: usize = 10000;

/// Sensitive headers that should be masked in logs
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "set-cookie",
    "cookie",
    "x-api-key",
    "x-auth-token",
    "proxy-authorization",
];

/// Global mutex for thread-safe log writing
static LOG_MUTEX: Mutex<()> = Mutex::new(());

/// Check if HTTP logging is enabled
pub fn is_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(ENV_HTTP_LOG)
            .map(|v| {
                let v = v.trim().to_lowercase();
                v == "1" || v == "true" || v == "yes" || v == "on"
            })
            .unwrap_or(false)
    })
}

/// Get log file path for a project
fn get_log_file_path(project_root: Option<&std::path::Path>) -> PathBuf {
    if let Some(root) = project_root {
        let ace_tool_dir = root.join(".ace-tool");
        if !ace_tool_dir.exists() {
            if let Err(e) = fs::create_dir_all(&ace_tool_dir) {
                warn!("Failed to create .ace-tool directory: {}", e);
            }
        }
        ace_tool_dir.join(LOG_FILE_NAME)
    } else {
        // Default to current directory's .ace-tool folder
        let ace_tool_dir = PathBuf::from(".ace-tool");
        if !ace_tool_dir.exists() {
            if let Err(e) = fs::create_dir_all(&ace_tool_dir) {
                warn!("Failed to create .ace-tool directory: {}", e);
            }
        }
        ace_tool_dir.join(LOG_FILE_NAME)
    }
}

/// HTTP request log entry
pub struct HttpRequestLog {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// HTTP response log entry
pub struct HttpResponseLog {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// Log an HTTP request and response
pub fn log_request(
    project_root: Option<&std::path::Path>,
    request: &HttpRequestLog,
    response: Option<&HttpResponseLog>,
    duration_ms: u64,
    error: Option<&str>,
) {
    if !is_enabled() {
        return;
    }

    let log_path = get_log_file_path(project_root);
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let separator = "=".repeat(80);

    let mut log_content = String::new();
    log_content.push_str(&format!(
        "\n{}\n[{}] {} {}\n{}\n",
        separator, timestamp, request.method, request.url, separator
    ));

    // Request headers
    log_content.push_str("\n--- Request Headers ---\n");
    for (name, value) in &request.headers {
        let display_value = mask_sensitive_header(name, value);
        log_content.push_str(&format!("{}: {}\n", name, display_value));
    }

    // Request body
    if let Some(body) = &request.body {
        log_content.push_str("\n--- Request Body ---\n");
        log_content.push_str(&format_body(body));
        log_content.push('\n');
    }

    // Response
    if let Some(resp) = response {
        log_content.push_str(&format!("\n--- Response ({}ms) ---\n", duration_ms));
        log_content.push_str(&format!("Status: {}\n", resp.status));

        log_content.push_str("\n--- Response Headers ---\n");
        for (name, value) in &resp.headers {
            let display_value = mask_sensitive_header(name, value);
            log_content.push_str(&format!("{}: {}\n", name, display_value));
        }

        if let Some(body) = &resp.body {
            log_content.push_str("\n--- Response Body ---\n");
            log_content.push_str(&format_body(body));
            log_content.push('\n');
        }
    }

    // Error
    if let Some(err) = error {
        log_content.push_str(&format!("\n--- Error ({}ms) ---\n", duration_ms));
        log_content.push_str(err);
        log_content.push('\n');
    }

    log_content.push_str(&format!("\n{}\n", "=".repeat(80)));

    // Write to file with mutex protection
    if let Err(e) = write_log(&log_path, &log_content) {
        warn!("Failed to write HTTP log: {}", e);
    }
}

/// Write log content to file (thread-safe)
fn write_log(path: &PathBuf, content: &str) -> std::io::Result<()> {
    // Acquire lock to prevent interleaved writes from concurrent requests
    let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Check if a header is sensitive and should be masked
fn is_sensitive_header(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    SENSITIVE_HEADERS.iter().any(|h| name_lower == *h)
}

/// Mask sensitive header values
fn mask_sensitive_header(name: &str, value: &str) -> String {
    if is_sensitive_header(name) {
        mask_token(value)
    } else {
        value.to_string()
    }
}

/// Mask authorization token for security
fn mask_token(value: &str) -> String {
    if let Some(token) = value.strip_prefix("Bearer ") {
        if token.len() > 8 {
            // Use char_indices for UTF-8 safe slicing
            let chars: Vec<char> = token.chars().collect();
            if chars.len() > 8 {
                let prefix: String = chars[..4].iter().collect();
                let suffix: String = chars[chars.len() - 4..].iter().collect();
                format!("Bearer {}...{}", prefix, suffix)
            } else {
                "Bearer ****".to_string()
            }
        } else {
            "Bearer ****".to_string()
        }
    } else if value.len() > 8 {
        // Generic token masking for non-Bearer tokens
        let chars: Vec<char> = value.chars().collect();
        if chars.len() > 8 {
            let prefix: String = chars[..4].iter().collect();
            let suffix: String = chars[chars.len() - 4..].iter().collect();
            format!("{}...{}", prefix, suffix)
        } else {
            "****".to_string()
        }
    } else {
        "****".to_string()
    }
}

/// Format body for logging with truncation (UTF-8 safe)
fn format_body(body: &str) -> String {
    // Try to parse and pretty-print JSON
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let pretty = serde_json::to_string_pretty(&json).unwrap_or_else(|_| body.to_string());
        // Truncate pretty-printed JSON if too large
        truncate_utf8_safe(&pretty, MAX_BODY_SIZE)
    } else {
        // Non-JSON body, truncate if needed
        truncate_utf8_safe(body, MAX_BODY_SIZE)
    }
}

/// Truncate string at UTF-8 character boundary (safe for multi-byte chars)
fn truncate_utf8_safe(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    // Find the last valid UTF-8 character boundary before max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...\n[truncated, total {} bytes]", &s[..end], s.len())
}

/// Helper to build request headers from reqwest RequestBuilder
/// Returns None if logging is disabled (for lazy evaluation)
#[allow(clippy::too_many_arguments)]
pub fn build_request_log_if_enabled(
    method: &str,
    url: &str,
    content_type: &str,
    user_agent: &str,
    request_id: &str,
    session_id: &str,
    auth_token: &str,
    body: Option<&str>,
) -> Option<HttpRequestLog> {
    if !is_enabled() {
        return None;
    }

    Some(HttpRequestLog {
        method: method.to_string(),
        url: url.to_string(),
        headers: vec![
            ("Content-Type".to_string(), content_type.to_string()),
            ("User-Agent".to_string(), user_agent.to_string()),
            ("x-request-id".to_string(), request_id.to_string()),
            ("x-request-session-id".to_string(), session_id.to_string()),
            (
                "Authorization".to_string(),
                format!("Bearer {}", auth_token),
            ),
        ],
        body: body.map(|s| s.to_string()),
    })
}

/// Extract headers from reqwest Response (only if logging enabled)
pub fn extract_response_headers(response: &reqwest::Response) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect()
}

/// Legacy helper for backward compatibility
pub fn extract_headers_from_builder(
    content_type: &str,
    user_agent: &str,
    request_id: &str,
    session_id: &str,
    auth_token: &str,
) -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_string(), content_type.to_string()),
        ("User-Agent".to_string(), user_agent.to_string()),
        ("x-request-id".to_string(), request_id.to_string()),
        ("x-request-session-id".to_string(), session_id.to_string()),
        (
            "Authorization".to_string(),
            format!("Bearer {}", auth_token),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
