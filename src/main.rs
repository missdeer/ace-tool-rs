//! ace-tool - MCP server for codebase indexing and semantic search

use ace_tool::config::Config;
use ace_tool::enhancer::prompt_enhancer::{get_enhancer_endpoint, PromptEnhancer};
use ace_tool::index::IndexManager;
use ace_tool::mcp::{McpServer, TransportMode};
use ace_tool::service::get_third_party_config;
use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use std::env;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(ValueEnum, Debug, Copy, Clone)]
enum TransportArg {
    Auto,
    Lsp,
    Line,
}

#[derive(Parser, Debug)]
#[command(name = "ace-tool")]
#[command(about = "MCP server for codebase indexing and semantic search")]
struct Args {
    /// API base URL for the indexing service
    #[arg(long)]
    base_url: Option<String>,

    /// Authentication token
    #[arg(long)]
    token: Option<String>,

    /// Transport framing: auto, lsp, line
    #[arg(long, value_enum, default_value = "auto")]
    transport: TransportArg,

    /// Maximum lines per blob (default: 800)
    #[arg(long)]
    max_lines_per_blob: Option<usize>,

    /// Upload timeout in seconds (default: adaptive)
    #[arg(long)]
    upload_timeout: Option<u64>,

    /// Upload concurrency (default: adaptive)
    #[arg(long)]
    upload_concurrency: Option<usize>,

    /// Retrieval timeout in seconds (default: 60)
    #[arg(long)]
    retrieval_timeout: Option<u64>,

    /// Disable adaptive strategy
    #[arg(long, default_value = "false")]
    no_adaptive: bool,

    /// Index-only mode: index current directory and exit (no MCP server)
    #[arg(long, default_value = "false")]
    index_only: bool,

    /// Enhance a prompt and output the result to stdout, then exit
    #[arg(long)]
    enhance_prompt: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for stderr (MCP uses stdout for protocol)
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Enhance-prompt mode: enhance the prompt and output to stdout
    if let Some(ref prompt) = args.enhance_prompt {
        info!("Enhance-prompt mode: enhancing prompt");
        let project_root = env::current_dir()?;
        info!("Project root: {:?}", project_root);

        // Check if using third-party endpoint (claude/openai/gemini)
        let endpoint = get_enhancer_endpoint();
        let config = if endpoint.is_third_party() {
            // For third-party endpoints, base_url and token are not required from CLI
            // They will be read from environment variables
            // Validate early that required environment variables are set
            let _ = get_third_party_config(endpoint)
                .map_err(|e| anyhow!("Third-party endpoint configuration error: {}", e))?;
            info!("Using third-party endpoint: {}", endpoint);
            Config::new_for_third_party_enhancer()
        } else {
            // For new/old endpoints, base_url and token are required
            let base_url = args.base_url.clone().ok_or_else(|| {
                anyhow!("--base-url is required for '{}' endpoint", endpoint)
            })?;
            let token = args.token.clone().ok_or_else(|| {
                anyhow!("--token is required for '{}' endpoint", endpoint)
            })?;
            Config::new(
                base_url,
                token,
                args.max_lines_per_blob,
                args.upload_timeout,
                args.upload_concurrency,
                args.retrieval_timeout,
                args.no_adaptive,
            )?
        };

        let enhancer = PromptEnhancer::new(config.clone())?;
        let enhanced = enhancer
            .enhance_simple(prompt, "", Some(&project_root))
            .await?;

        // Output enhanced prompt to stdout
        println!("{}", enhanced);
        return Ok(());
    }

    // For non-enhance-prompt modes, base_url and token are always required
    let base_url = args.base_url.ok_or_else(|| {
        anyhow!("--base-url is required")
    })?;
    let token = args.token.ok_or_else(|| {
        anyhow!("--token is required")
    })?;

    // Initialize configuration
    let config = Config::new(
        base_url,
        token,
        args.max_lines_per_blob,
        args.upload_timeout,
        args.upload_concurrency,
        args.retrieval_timeout,
        args.no_adaptive,
    )?;

    // Index-only mode: index current directory and exit
    if args.index_only {
        info!("Index-only mode: indexing current directory");
        let project_root = env::current_dir()?;
        info!("Project root: {:?}", project_root);

        let manager = IndexManager::new(config, project_root)?;
        let result = manager.index_project().await;

        match result.status.as_str() {
            "success" => {
                info!("Indexing completed successfully: {}", result.message);
                if let Some(stats) = result.stats {
                    info!(
                        "Stats: {} total blobs, {} existing, {} new",
                        stats.total_blobs, stats.existing_blobs, stats.new_blobs
                    );
                }
                return Ok(());
            }
            "partial" => {
                warn!("Indexing completed with warnings: {}", result.message);
                if let Some(stats) = result.stats {
                    if let Some(failed_batches) = stats.failed_batches {
                        warn!(
                            "Stats: {} total blobs, {} existing, {} new, {} failed batches",
                            stats.total_blobs,
                            stats.existing_blobs,
                            stats.new_blobs,
                            failed_batches
                        );
                    } else {
                        warn!(
                            "Stats: {} total blobs, {} existing, {} new",
                            stats.total_blobs, stats.existing_blobs, stats.new_blobs
                        );
                    }
                }
                std::process::exit(2);
            }
            _ => {
                return Err(anyhow::anyhow!("Indexing failed: {}", result.message));
            }
        }
    }

    info!("Starting ace-tool MCP server");

    let transport_mode = match args.transport {
        TransportArg::Auto => None,
        TransportArg::Lsp => Some(TransportMode::Lsp),
        TransportArg::Line => Some(TransportMode::Line),
    };

    // Create and run MCP server
    let server = McpServer::new(config, transport_mode);

    if let Err(e) = server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
