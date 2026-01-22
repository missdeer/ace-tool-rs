//! MCP server implementation

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::tools::enhance_prompt::{EnhancePromptArgs, EnhancePromptToolDef, ENHANCE_PROMPT_TOOL};
use crate::tools::search_context::{SearchContextArgs, SearchContextToolDef, SEARCH_CONTEXT_TOOL};
use crate::tools::{EnhancePromptTool, SearchContextTool};

/// Map tool name aliases to canonical names
fn normalize_tool_name(name: &str) -> &str {
    match name {
        "codebase-retrieval" => "search_context",
        _ => name,
    }
}

use super::types::*;

/// Check if the enhance_prompt tool is enabled.
/// The tool is disabled when PROMPT_ENHANCER env var is set to "disabled", "false", "0", or "off".
/// By default (env var not set or set to other values), the tool is enabled.
fn is_enhance_prompt_enabled() -> bool {
    std::env::var("PROMPT_ENHANCER")
        .map(|v| {
            let v = v.trim();
            !v.eq_ignore_ascii_case("disabled")
                && !v.eq_ignore_ascii_case("false")
                && !v.eq_ignore_ascii_case("off")
                && v != "0"
        })
        .unwrap_or(true)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TransportMode {
    Lsp,
    Line,
}

pub fn is_header_line(line: &str) -> bool {
    match line.split_once(':') {
        Some((name, _)) => {
            let name = name.trim();
            name.eq_ignore_ascii_case("content-length") || name.eq_ignore_ascii_case("content-type")
        }
        None => false,
    }
}

pub fn parse_content_length(line: &str) -> Result<Option<usize>> {
    let (name, value) = match line.split_once(':') {
        Some(parts) => parts,
        None => return Ok(None),
    };

    if !name.trim().eq_ignore_ascii_case("content-length") {
        return Ok(None);
    }

    let length = value
        .trim()
        .parse::<usize>()
        .map_err(|e| anyhow!("Invalid Content-Length header: {}", e))?;
    Ok(Some(length))
}

/// Maximum line length for Line mode to prevent DoS (10MB)
const MAX_LINE_LENGTH: usize = 10 * 1024 * 1024;

async fn read_line_message(reader: &mut BufReader<tokio::io::Stdin>) -> Result<Option<String>> {
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }

        // Protect against DoS from extremely long lines
        if line.len() > MAX_LINE_LENGTH {
            return Err(anyhow!(
                "Line length {} exceeds maximum allowed size of {} bytes",
                line.len(),
                MAX_LINE_LENGTH
            ));
        }

        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            continue;
        }

        return Ok(Some(trimmed.to_string()));
    }
}

/// Maximum header line length for LSP mode to prevent DoS (1KB should be enough for headers)
const MAX_HEADER_LENGTH: usize = 1024;
/// Maximum number of header lines (including skipped blank lines) to prevent DoS
pub const MAX_HEADER_COUNT: usize = 100;

async fn read_lsp_message(
    reader: &mut BufReader<tokio::io::Stdin>,
    first_line: Option<String>,
) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;
    let mut pending = first_line;
    let mut seen_header = false;
    let mut line_count = 0;

    loop {
        let line = if let Some(line) = pending.take() {
            line
        } else {
            let mut header = String::new();
            let bytes = reader.read_line(&mut header).await?;
            if bytes == 0 {
                return Ok(None);
            }
            // Protect against DoS from extremely long header lines
            if header.len() > MAX_HEADER_LENGTH {
                return Err(anyhow!(
                    "Header line length {} exceeds maximum allowed size of {} bytes",
                    header.len(),
                    MAX_HEADER_LENGTH
                ));
            }
            header.trim_end_matches(&['\r', '\n'][..]).to_string()
        };

        // Protect against DoS from infinite headers or blank lines
        line_count += 1;
        if line_count > MAX_HEADER_COUNT {
            return Err(anyhow!(
                "Too many header lines or skipped blank lines (limit {})",
                MAX_HEADER_COUNT
            ));
        }

        if line.is_empty() {
            // Skip leading blank lines; break only after seeing at least one header
            if seen_header {
                break;
            }
            continue;
        }

        seen_header = true;
        if let Some(len) = parse_content_length(&line)? {
            content_length = Some(len);
        }
    }

    let length =
        content_length.ok_or_else(|| anyhow!("Missing Content-Length header in LSP message"))?;

    // Limit Content-Length to 10MB to prevent DoS from malicious headers
    const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
    if length > MAX_MESSAGE_SIZE {
        return Err(anyhow!(
            "Content-Length {} exceeds maximum allowed size of {} bytes",
            length,
            MAX_MESSAGE_SIZE
        ));
    }

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await?;
    let message = String::from_utf8(buf).map_err(|e| anyhow!("Invalid UTF-8 payload: {}", e))?;
    Ok(Some(message))
}

async fn read_message(
    reader: &mut BufReader<tokio::io::Stdin>,
    mode: &mut Option<TransportMode>,
) -> Result<Option<String>> {
    match mode {
        Some(TransportMode::Line) => read_line_message(reader).await,
        Some(TransportMode::Lsp) => read_lsp_message(reader, None).await,
        None => loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).await?;
            if bytes == 0 {
                return Ok(None);
            }

            // Protect against DoS from extremely long lines during auto-detection
            if line.len() > MAX_LINE_LENGTH {
                return Err(anyhow!(
                    "Line length {} exceeds maximum allowed size of {} bytes",
                    line.len(),
                    MAX_LINE_LENGTH
                ));
            }

            let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
            if trimmed.is_empty() {
                continue;
            }

            if parse_content_length(trimmed)?.is_some() || is_header_line(trimmed) {
                *mode = Some(TransportMode::Lsp);
                return read_lsp_message(reader, Some(trimmed.to_string())).await;
            }

            *mode = Some(TransportMode::Line);
            return Ok(Some(trimmed.to_string()));
        },
    }
}

async fn write_message(
    stdout: &mut tokio::io::Stdout,
    mode: TransportMode,
    payload: &str,
) -> Result<()> {
    let mut buffer = Vec::new();

    match mode {
        TransportMode::Line => {
            buffer.extend_from_slice(payload.as_bytes());
            buffer.push(b'\n');
        }
        TransportMode::Lsp => {
            let header = format!("Content-Length: {}\r\n\r\n", payload.len());
            buffer.extend_from_slice(header.as_bytes());
            buffer.extend_from_slice(payload.as_bytes());
        }
    }

    stdout.write_all(&buffer).await?;
    stdout.flush().await?;
    Ok(())
}

/// MCP Server
pub struct McpServer {
    config: Arc<Config>,
    initial_transport_mode: Option<TransportMode>,
    active_transport_mode: Arc<RwLock<Option<TransportMode>>>,
}

impl McpServer {
    pub fn new(config: Arc<Config>, transport_mode: Option<TransportMode>) -> Self {
        Self {
            config,
            initial_transport_mode: transport_mode,
            active_transport_mode: Arc::new(RwLock::new(transport_mode)),
        }
    }

    /// Run the MCP server (stdio transport)
    pub async fn run(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut transport_mode = self.initial_transport_mode;

        info!("MCP server started, waiting for requests...");

        loop {
            let message = match read_message(&mut reader, &mut transport_mode).await {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(e) => {
                    error!("Failed to read message: {}", e);
                    continue;
                }
            };

            if message.is_empty() {
                continue;
            }

            // Update the shared transport mode when auto-detection determines it
            if transport_mode.is_some() {
                let mut active = self.active_transport_mode.write().await;
                if active.is_none() {
                    *active = transport_mode;
                }
            }

            debug!("Received: {}", message);

            match serde_json::from_str::<JsonRpcRequest>(&message) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    if let Some(resp) = response {
                        let resp_json = serde_json::to_string(&resp)?;
                        debug!("Sending: {}", resp_json);
                        let mode = transport_mode.unwrap_or(TransportMode::Line);
                        write_message(&mut stdout, mode, &resp_json).await?;
                    }
                }
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let error_response =
                        JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                    let resp_json = serde_json::to_string(&error_response)?;
                    let mode = transport_mode.unwrap_or(TransportMode::Line);
                    write_message(&mut stdout, mode, &resp_json).await?;
                }
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Per JSON-RPC spec, requests without an id are notifications and must not receive a response
        if request.id.is_none() {
            // Handle known notification side effects silently
            match request.method.as_str() {
                "initialized" | "notifications/initialized" => {
                    // Client initialization complete - no action needed
                }
                _ => {
                    // Unknown notification - log and ignore per JSON-RPC spec
                    debug!("Received notification: {}", request.method);
                }
            }
            return None;
        }

        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(request.id)),
            "initialized" => None, // Notification, no response
            "tools/list" => Some(self.handle_list_tools(request.id)),
            "tools/call" => Some(self.handle_call_tool(request.id, request.params).await),
            "ping" => Some(JsonRpcResponse::success(request.id, json!({}))),
            _ => Some(JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            )),
        }
    }

    /// Handle initialize request
    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {}),
                logging: None,
            },
            server_info: ServerInfo {
                name: "ace-tool".to_string(),
                version: "0.1.8".to_string(),
            },
        };

        match serde_json::to_value(result) {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(e) => JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e)),
        }
    }

    /// Handle list tools request
    fn handle_list_tools(&self, id: Option<Value>) -> JsonRpcResponse {
        let mut tools = vec![Tool {
            name: SEARCH_CONTEXT_TOOL.name.to_string(),
            description: SEARCH_CONTEXT_TOOL.description.to_string(),
            input_schema: SearchContextToolDef::get_input_schema(),
        }];

        // Only expose enhance_prompt tool if not disabled
        if is_enhance_prompt_enabled() {
            tools.push(Tool {
                name: ENHANCE_PROMPT_TOOL.name.to_string(),
                description: ENHANCE_PROMPT_TOOL.description.to_string(),
                input_schema: EnhancePromptToolDef::get_input_schema(),
            });
        }

        let result = ListToolsResult { tools };

        match serde_json::to_value(result) {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(e) => JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e)),
        }
    }

    /// Handle tool call request
    async fn handle_call_tool(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
            }
        };

        let call_params: CallToolParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return JsonRpcResponse::error(id, -32602, format!("Invalid params: {}", e));
            }
        };

        let tool_name = normalize_tool_name(&call_params.name);

        match tool_name {
            "search_context" => {
                let args: SearchContextArgs = match call_params.arguments {
                    Some(args) => match serde_json::from_value(args) {
                        Ok(a) => a,
                        Err(e) => {
                            return JsonRpcResponse::error(
                                id,
                                -32602,
                                format!("Invalid arguments: {}", e),
                            );
                        }
                    },
                    None => SearchContextArgs::default(),
                };

                let tool = SearchContextTool::new(self.config.clone());
                let result = tool.execute(args).await;

                let call_result = CallToolResult {
                    content: vec![TextContent::new(result.text)],
                };

                match serde_json::to_value(call_result) {
                    Ok(value) => JsonRpcResponse::success(id, value),
                    Err(e) => JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e)),
                }
            }
            "enhance_prompt" => {
                // Check if the tool is enabled before executing
                if !is_enhance_prompt_enabled() {
                    return JsonRpcResponse::error(
                        id,
                        -32602,
                        "Tool 'enhance_prompt' is disabled".to_string(),
                    );
                }

                let args: EnhancePromptArgs = match call_params.arguments {
                    Some(args) => match serde_json::from_value(args) {
                        Ok(a) => a,
                        Err(e) => {
                            return JsonRpcResponse::error(
                                id,
                                -32602,
                                format!("Invalid arguments: {}", e),
                            );
                        }
                    },
                    None => EnhancePromptArgs::default(),
                };

                let tool = EnhancePromptTool::new(self.config.clone());
                let result = tool.execute(args).await;

                let call_result = CallToolResult {
                    content: vec![TextContent::new(result.text)],
                };

                match serde_json::to_value(call_result) {
                    Ok(value) => JsonRpcResponse::success(id, value),
                    Err(e) => JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e)),
                }
            }
            _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {}", call_params.name)),
        }
    }

    /// Send a log notification to the client
    #[allow(dead_code)]
    pub async fn send_log(&self, level: &str, message: &str) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/message".to_string(),
            params: serde_json::to_value(LoggingMessageParams {
                level: level.to_string(),
                data: message.to_string(),
            })?,
        };

        let mut stdout = tokio::io::stdout();
        let json = serde_json::to_string(&notification)?;
        let mode = self
            .active_transport_mode
            .read()
            .await
            .or(self.initial_transport_mode)
            .unwrap_or(TransportMode::Line);
        write_message(&mut stdout, mode, &json).await?;

        Ok(())
    }
}
