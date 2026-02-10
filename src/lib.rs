//! ace-tool library - MCP server for codebase indexing and semantic search

pub mod config;
pub mod enhancer;
pub mod http_logger;
pub mod index;
pub mod mcp;
pub mod service;
pub mod strategy;
pub mod tools;
pub mod utils;

/// User-Agent header value (matches augment.mjs format: augment.cli/{version})
pub const USER_AGENT: &str = "augment.cli/0.12.0";

// Re-export commonly used types
pub use config::{get_upload_strategy, CliOverrides, Config, UploadStrategy};
pub use enhancer::PromptEnhancer;
pub use index::{Blob, IndexManager, IndexResult, IndexStats};
