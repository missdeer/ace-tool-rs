//! MCP server implementation

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::tools::search_context::{SearchContextArgs, SearchContextToolDef, SEARCH_CONTEXT_TOOL};
use crate::tools::SearchContextTool;

use super::types::*;

/// MCP Server
pub struct McpServer {
    config: Arc<Config>,
}

impl McpServer {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Run the MCP server (stdio transport)
    pub async fn run(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        info!("MCP server started, waiting for requests...");

        while let Some(line) = lines.next_line().await? {
            if line.is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    if let Some(resp) = response {
                        let resp_json = serde_json::to_string(&resp)?;
                        debug!("Sending: {}", resp_json);
                        stdout.write_all(resp_json.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                    }
                }
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let error_response =
                        JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                    let resp_json = serde_json::to_string(&error_response)?;
                    stdout.write_all(resp_json.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
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
                logging: Some(LoggingCapability {}),
            },
            server_info: ServerInfo {
                name: "ace-tool".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        match serde_json::to_value(result) {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(e) => JsonRpcResponse::error(id, -32603, format!("Internal error: {}", e)),
        }
    }

    /// Handle list tools request
    fn handle_list_tools(&self, id: Option<Value>) -> JsonRpcResponse {
        let result = ListToolsResult {
            tools: vec![Tool {
                name: SEARCH_CONTEXT_TOOL.name.to_string(),
                description: SEARCH_CONTEXT_TOOL.description.to_string(),
                input_schema: SearchContextToolDef::get_input_schema(),
            }],
        };

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

        match call_params.name.as_str() {
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
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;

        Ok(())
    }
}
