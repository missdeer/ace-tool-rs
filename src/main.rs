//! ace-tool - MCP server for codebase indexing and semantic search

use ace_tool::config::Config;
use ace_tool::mcp::McpServer;
use anyhow::Result;
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "ace-tool")]
#[command(about = "MCP server for codebase indexing and semantic search")]
struct Args {
    /// API base URL for the indexing service
    #[arg(long)]
    base_url: String,

    /// Authentication token
    #[arg(long)]
    token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for stderr (MCP uses stdout for protocol)
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Initialize configuration
    let config = Config::new(args.base_url, args.token)?;

    info!("Starting ace-tool MCP server");

    // Create and run MCP server
    let server = McpServer::new(config);

    if let Err(e) = server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
