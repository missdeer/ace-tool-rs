//! MCP (Model Context Protocol) module

mod server;
pub mod types;

pub use server::{McpServer, TransportMode};
