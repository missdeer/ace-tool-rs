//! MCP (Model Context Protocol) module

pub mod server;
pub mod types;

pub use server::{
    is_header_line, parse_content_length, McpServer, TransportMode, MAX_HEADER_COUNT,
};
