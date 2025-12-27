//! ace-tool library - MCP server for codebase indexing and semantic search

pub mod config;
pub mod index;
pub mod mcp;
pub mod tools;
pub mod utils;

// Re-export commonly used types
pub use config::{get_upload_strategy, Config, UploadStrategy};
pub use index::{Blob, IndexManager, IndexResult, IndexStats};
