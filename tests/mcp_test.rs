//! Tests for MCP types module

use ace_tool::mcp::types::*;
use serde_json::json;

#[test]
fn test_json_rpc_request_serialization() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "tools/list".to_string(),
        params: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"tools/list\""));

    let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.method, "tools/list");
}

#[test]
fn test_json_rpc_request_with_params() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(42)),
        method: "tools/call".to_string(),
        params: Some(json!({"name": "search_context"})),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("search_context"));
}

#[test]
fn test_json_rpc_response_success() {
    let response = JsonRpcResponse::success(Some(json!(1)), json!({"status": "ok"}));

    assert_eq!(response.jsonrpc, "2.0");
    assert!(response.result.is_some());
    assert!(response.error.is_none());

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"result\""));
    assert!(!json.contains("\"error\""));
}

#[test]
fn test_json_rpc_response_error() {
    let response = JsonRpcResponse::error(Some(json!(1)), -32601, "Method not found".to_string());

    assert_eq!(response.jsonrpc, "2.0");
    assert!(response.result.is_none());
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    assert_eq!(error.code, -32601);
    assert_eq!(error.message, "Method not found");
}

#[test]
fn test_json_rpc_response_serialization() {
    let response = JsonRpcResponse::success(Some(json!(1)), json!({"data": "test"}));
    let json = serde_json::to_string(&response).unwrap();

    let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.jsonrpc, "2.0");
    assert!(deserialized.result.is_some());
}

#[test]
fn test_server_capabilities() {
    let caps = ServerCapabilities {
        tools: Some(ToolsCapability {}),
        logging: Some(LoggingCapability {}),
    };

    let json = serde_json::to_string(&caps).unwrap();
    assert!(json.contains("\"tools\""));
    assert!(json.contains("\"logging\""));
}

#[test]
fn test_server_info() {
    let info = ServerInfo {
        name: "ace-tool".to_string(),
        version: "0.1.0".to_string(),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("ace-tool"));
    assert!(json.contains("0.1.0"));
}

#[test]
fn test_initialize_result() {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {}),
            logging: None,
        },
        server_info: ServerInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"protocolVersion\":\"2024-11-05\""));
    assert!(json.contains("\"serverInfo\""));
}

#[test]
fn test_tool_definition() {
    let tool = Tool {
        name: "search_context".to_string(),
        description: "Search the codebase".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("\"inputSchema\""));
    assert!(json.contains("search_context"));
}

#[test]
fn test_list_tools_result() {
    let result = ListToolsResult {
        tools: vec![
            Tool {
                name: "tool1".to_string(),
                description: "First tool".to_string(),
                input_schema: json!({}),
            },
            Tool {
                name: "tool2".to_string(),
                description: "Second tool".to_string(),
                input_schema: json!({}),
            },
        ],
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("tool1"));
    assert!(json.contains("tool2"));
}

#[test]
fn test_call_tool_params() {
    let params = CallToolParams {
        name: "search_context".to_string(),
        arguments: Some(json!({"query": "find auth"})),
    };

    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("search_context"));
    assert!(json.contains("find auth"));

    let deserialized: CallToolParams = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "search_context");
}

#[test]
fn test_text_content_new() {
    let content = TextContent::new("Hello, World!".to_string());

    assert_eq!(content.content_type, "text");
    assert_eq!(content.text, "Hello, World!");
}

#[test]
fn test_text_content_serialization() {
    let content = TextContent::new("Test message".to_string());

    let json = serde_json::to_string(&content).unwrap();
    assert!(json.contains("\"type\":\"text\""));
    assert!(json.contains("Test message"));
}

#[test]
fn test_call_tool_result() {
    let result = CallToolResult {
        content: vec![
            TextContent::new("Result 1".to_string()),
            TextContent::new("Result 2".to_string()),
        ],
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("Result 1"));
    assert!(json.contains("Result 2"));
}

#[test]
fn test_logging_message_params() {
    let params = LoggingMessageParams {
        level: "info".to_string(),
        data: "Log message".to_string(),
    };

    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"level\":\"info\""));
    assert!(json.contains("Log message"));
}

#[test]
fn test_json_rpc_notification() {
    let notification = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "notifications/message".to_string(),
        params: json!({"level": "info", "data": "test"}),
    };

    let json = serde_json::to_string(&notification).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("notifications/message"));
    // Should not have id field
    assert!(!json.contains("\"id\""));
}

#[test]
fn test_json_rpc_error_codes() {
    // Parse error
    let response = JsonRpcResponse::error(None, -32700, "Parse error".to_string());
    assert_eq!(response.error.as_ref().unwrap().code, -32700);

    // Invalid request
    let response = JsonRpcResponse::error(None, -32600, "Invalid Request".to_string());
    assert_eq!(response.error.as_ref().unwrap().code, -32600);

    // Method not found
    let response = JsonRpcResponse::error(None, -32601, "Method not found".to_string());
    assert_eq!(response.error.as_ref().unwrap().code, -32601);

    // Invalid params
    let response = JsonRpcResponse::error(None, -32602, "Invalid params".to_string());
    assert_eq!(response.error.as_ref().unwrap().code, -32602);
}
