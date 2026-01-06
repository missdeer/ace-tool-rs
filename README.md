# ace-tool-rs

A high-performance MCP (Model Context Protocol) server for codebase indexing, semantic search, and prompt enhancement, written in Rust.

## Overview

ace-tool-rs is a Rust implementation of a codebase context engine that enables AI assistants to search and understand codebases using natural language queries. It provides:

- **Real-time codebase indexing** - Automatically indexes your project files and keeps the index up-to-date
- **Semantic search** - Find relevant code using natural language descriptions
- **Prompt enhancement** - Enhance user prompts with codebase context for clearer, more actionable requests
- **Multi-language support** - Works with 50+ programming languages and file types
- **Incremental updates** - Only uploads changed files to minimize network overhead
- **Smart exclusions** - Respects `.gitignore` and common ignore patterns

## Features

- **MCP Protocol Support** - Full JSON-RPC 2.0 implementation over stdio transport
- **Adaptive Upload Strategy** - Automatically adjusts batch size and concurrency based on project size
- **Multi-encoding Support** - Handles UTF-8, GBK, GB18030, and Windows-1252 encoded files
- **Concurrent Uploads** - Parallel batch uploads for faster indexing of large projects
- **Robust Error Handling** - Retry logic with exponential backoff and rate limiting support

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/missdeer/ace-tool-rs.git
cd ace-tool-rs

# Build release binary
cargo build --release

# The binary will be at target/release/ace-tool-rs
```

### Requirements

- Rust 1.70 or later
- An API endpoint for the indexing service
- Authentication token

## Usage

### Command Line

```bash
ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `--base-url` | API base URL for the indexing service |
| `--token` | Authentication token for API access |
| `--transport` | Transport framing: `auto` (default), `lsp`, `line` |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Set log level (e.g., `info`, `debug`, `warn`) |
| `ACE_ENHANCER_ENDPOINT` | Endpoint selection: `new` (default, uses `/prompt-enhancer`) or `old` (uses `/chat-stream`) |

### Example

```bash
# Run with debug logging
RUST_LOG=debug ace-tool-rs --base-url https://api.example.com --token your-token-here
```

### Transport Framing

By default, the server auto-detects line-delimited JSON vs. LSP `Content-Length` framing.
If your client requires a specific mode, force it:

```bash
ace-tool-rs --base-url https://api.example.com --token your-token-here --transport lsp
```

## MCP Integration

### Codex CLI Configuration

Add to your Codex config file (typically `~/.codex/config.toml`):

```toml
[mcp_servers.ace-tool]
command = "/path/to/ace-tool-rs"
args = ["--base-url", "https://api.example.com", "--token", "your-token-here", "--transport", "lsp"]
env = { RUST_LOG = "info" }
startup_timeout_ms = 60000
```

### Claude Desktop Configuration

Add to your Claude Desktop configuration file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/path/to/ace-tool-rs",
      "args": [
        "--base-url", "https://api.example.com",
        "--token", "your-token-here"
      ]
    }
  }
}
```

### Claude Code

Run command like below:

```bash
claude mcp add-json ace-tool --scope user '{"type":"stdio","command":"/path/to/ace-tool-rs","args":["--base-url",  "https://api.example.com/",  "--token", "your-token-here"],"env":{}}'
```

Modify `~/.claude/settings.json` to add permission for the tools:

```json
$ cat settings.local.json
{
  "permissions": {
    "allow": [
      "mcp__ace-tool__search_context",
      "mcp__ace-tool__enhance_prompt"
    ]
  }
}
```

### Available Tools

#### `search_context`

Search the codebase using natural language queries.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_root_path` | string | Yes | Absolute path to the project root directory |
| `query` | string | Yes | Natural language description of the code you're looking for |

**Example queries:**

- "Where is the function that handles user authentication?"
- "What tests are there for the login functionality?"
- "How is the database connected to the application?"
- "Find the initialization flow of message queue consumers"

#### `enhance_prompt`

Enhance user prompts by combining codebase context and conversation history to generate clearer, more specific, and actionable prompts.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `prompt` | string | Yes | The original prompt to enhance |
| `conversation_history` | string | Yes | Recent conversation history (5-10 rounds) in format: `User: xxx\nAssistant: yyy` |
| `project_root_path` | string | No | Absolute path to the project root directory (optional, defaults to current working directory) |

**Features:**

- Automatic language detection (Chinese input → Chinese output, English input → English output)
- Uses codebase context from indexed files
- Considers conversation history for better context understanding

**API Endpoints:**

The tool supports two backend endpoints, controlled by the `ACE_ENHANCER_ENDPOINT` environment variable:

| Endpoint | Path | Description |
|----------|------|-------------|
| `new` (default) | `/prompt-enhancer` | Simplified request format, recommended |
| `old` | `/chat-stream` | Full request with blobs, streaming response |

## Supported File Types

### Programming Languages

`.py`, `.js`, `.ts`, `.jsx`, `.tsx`, `.java`, `.go`, `.rs`, `.cpp`, `.c`, `.h`, `.cs`, `.rb`, `.php`, `.swift`, `.kt`, `.scala`, `.lua`, `.dart`, `.r`, `.jl`, `.ex`, `.hs`, `.zig`, and many more.

### Configuration & Data

`.json`, `.yaml`, `.yml`, `.toml`, `.xml`, `.ini`, `.conf`, `.md`, `.txt`

### Web Technologies

`.html`, `.css`, `.scss`, `.sass`, `.vue`, `.svelte`, `.astro`

### Special Files

`Makefile`, `Dockerfile`, `Jenkinsfile`, `.gitignore`, `.env.example`, `requirements.txt`, and more.

## Default Exclusions

The following patterns are excluded by default:

- **Dependencies**: `node_modules`, `vendor`, `.venv`, `venv`
- **Build artifacts**: `target`, `dist`, `build`, `out`, `.next`
- **Version control**: `.git`, `.svn`, `.hg`
- **Cache directories**: `__pycache__`, `.cache`, `.pytest_cache`
- **Binary files**: `*.exe`, `*.dll`, `*.so`, `*.pyc`
- **Media files**: `*.png`, `*.jpg`, `*.mp4`, `*.pdf`
- **Lock files**: `package-lock.json`, `yarn.lock`, `Cargo.lock`

## Architecture

```
ace-tool-rs/
├── src/
│   ├── main.rs          # Entry point and CLI
│   ├── lib.rs           # Library exports
│   ├── config.rs        # Configuration and upload strategies
│   ├── index/
│   │   ├── mod.rs
│   │   └── manager.rs   # Core indexing and search logic
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── server.rs    # MCP server implementation
│   │   └── types.rs     # JSON-RPC types
│   ├── tools/
│   │   ├── mod.rs
│   │   └── search_context.rs  # Search tool implementation
│   └── utils/
│       ├── mod.rs
│       └── project_detector.rs  # Project utilities
└── tests/               # Integration tests
    ├── config_test.rs
    ├── index_test.rs
    ├── mcp_test.rs
    ├── tools_test.rs
    └── utils_test.rs
```

## Project Scale Strategies

The tool automatically adapts its upload strategy based on project size:

| Scale | Blob Count | Batch Size | Concurrency | Timeout |
|-------|------------|------------|-------------|---------|
| Small | < 100 | 10 | 1 | 30s |
| Medium | 100-499 | 30 | 2 | 45s |
| Large | 500-1999 | 50 | 3 | 60s |
| Extra Large | 2000+ | 70 | 4 | 90s |

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_config_new
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Check without building
cargo check

# Run clippy lints
cargo clippy
```

### Code Structure

- **69 unit tests** covering all major components
- Modular architecture with clear separation of concerns
- Async/await throughout using Tokio runtime
- Comprehensive error handling with `anyhow`

## Limitations

- Only processes the root `.gitignore` file (nested `.gitignore` files are not supported)
- Requires network access to the indexing API
- Maximum file size: 500KB per file
- Maximum batch size: 5MB per upload batch

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

[missdeer](https://github.com/missdeer)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
