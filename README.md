# ace-tool-rs

English | [简体中文](README-zh-CN.md)

A high-performance MCP (Model Context Protocol) server for codebase indexing, semantic search, and prompt enhancement, written in Rust.

## Overview

ace-tool-rs is a Rust implementation of a codebase context engine that enables AI assistants to search and understand codebases using natural language queries. It provides:

- **Real-time codebase indexing** - Automatically indexes your project files and keeps the index up-to-date
- **Semantic search** - Find relevant code using natural language descriptions
- **Prompt enhancement** - Enhance user prompts with codebase context for clearer, more actionable requests
- **Multi-language support** - Works with 50+ programming languages and file types
- **Incremental updates** - Uses mtime caching to skip unchanged files and only uploads new/modified content
- **Parallel processing** - Multi-threaded file scanning and processing for faster indexing
- **Smart exclusions** - Respects `.gitignore` and common ignore patterns

## Features

- **MCP Protocol Support** - Full JSON-RPC 2.0 implementation over stdio transport
- **Adaptive Upload Strategy** - AIMD (Additive Increase, Multiplicative Decrease) algorithm dynamically adjusts concurrency and timeout based on runtime metrics
- **Multi-encoding Support** - Handles UTF-8, GBK, GB18030, and Windows-1252 encoded files
- **Concurrent Uploads** - Parallel batch uploads with sliding window for faster indexing of large projects
- **Mtime Caching** - Tracks file modification times to avoid re-processing unchanged files
- **Robust Error Handling** - Retry logic with exponential backoff and rate limiting support

## Installation

### Quick Start (Recommended)

The easiest way to install and run ace-tool-rs is via npx:

```bash
npx ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

This will automatically download the appropriate binary for your platform and run it.

**Supported platforms:**
- Windows (x64)
- macOS (x64, ARM64)
- Linux (x64, ARM64)

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
| `--base-url` | API base URL for the indexing service (optional for `--enhance-prompt` with third-party endpoints) |
| `--token` | Authentication token for API access (optional for `--enhance-prompt` with third-party endpoints) |
| `--transport` | Transport framing: `auto` (default), `lsp`, `line` |
| `--upload-timeout` | Override upload timeout in seconds (disables adaptive timeout) |
| `--upload-concurrency` | Override upload concurrency (disables adaptive concurrency) |
| `--no-adaptive` | Disable adaptive strategy, use static heuristic values |
| `--index-only` | Index current directory and exit (no MCP server) |
| `--enhance-prompt` | Enhance a prompt and output the result to stdout, then exit |
| `--max-lines-per-blob` | Maximum lines per blob chunk (default: 800) |
| `--retrieval-timeout` | Search retrieval timeout in seconds (default: 180) |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Set log level (e.g., `info`, `debug`, `warn`) |
| `ACE_ENHANCER_ENDPOINT` | Endpoint selection: `new` (default), `old`, `claude`, `openai`, or `gemini` |
| `PROMPT_ENHANCER_BASE_URL` | Base URL for third-party API (required for `claude`/`openai`/`gemini`) |
| `PROMPT_ENHANCER_TOKEN` | API key for third-party API (required for `claude`/`openai`/`gemini`) |
| `PROMPT_ENHANCER_MODEL` | Model name override for third-party API (optional) |

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
command = "npx"
args = ["ace-tool-rs", "--base-url", "https://api.example.com", "--token", "your-token-here", "--transport", "lsp"]
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
      "command": "npx",
      "args": [
        "ace-tool-rs",
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
claude mcp add-json ace-tool --scope user '{"type":"stdio","command":"npx","args":["ace-tool-rs","--base-url","https://api.example.com/","--token","your-token-here"],"env":{}}'
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

The tool supports multiple backend endpoints, controlled by the `ACE_ENHANCER_ENDPOINT` environment variable:

| Endpoint | Description | Configuration |
|----------|-------------|---------------|
| `new` (default) | Augment `/prompt-enhancer` endpoint | Uses `--base-url` and `--token` CLI args |
| `old` | Augment `/chat-stream` endpoint (streaming) | Uses `--base-url` and `--token` CLI args |
| `claude` | Claude API (Anthropic) | Uses `PROMPT_ENHANCER_*` env vars |
| `openai` | OpenAI API | Uses `PROMPT_ENHANCER_*` env vars |
| `gemini` | Gemini API (Google) | Uses `PROMPT_ENHANCER_*` env vars |

**Default Models for Third-Party APIs:**

| Provider | Default Model |
|----------|---------------|
| Claude | `claude-sonnet-4-20250514` |
| OpenAI | `gpt-4o` |
| Gemini | `gemini-2.0-flash-exp` |

**Example using Claude API:**

```bash
# For MCP server mode, --base-url and --token are still required
export ACE_ENHANCER_ENDPOINT=claude
export PROMPT_ENHANCER_BASE_URL=https://api.anthropic.com
export PROMPT_ENHANCER_TOKEN=your-anthropic-api-key
ace-tool-rs --base-url https://api.example.com --token your-token

# For --enhance-prompt mode with third-party endpoints, --base-url and --token are optional
export ACE_ENHANCER_ENDPOINT=claude
export PROMPT_ENHANCER_BASE_URL=https://api.anthropic.com
export PROMPT_ENHANCER_TOKEN=your-anthropic-api-key
ace-tool-rs --enhance-prompt "Add user authentication"
```

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
│   ├── enhancer/
│   │   ├── mod.rs
│   │   ├── prompt_enhancer.rs  # Prompt enhancement orchestration
│   │   └── templates.rs        # Enhancement prompt templates
│   ├── index/
│   │   ├── mod.rs
│   │   └── manager.rs   # Core indexing and search logic
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── server.rs    # MCP server implementation
│   │   └── types.rs     # JSON-RPC types
│   ├── service/
│   │   ├── mod.rs       # Service module exports
│   │   ├── common.rs    # Shared types and utilities
│   │   ├── augment.rs   # Augment New/Old endpoints
│   │   ├── claude.rs    # Claude API (Anthropic)
│   │   ├── openai.rs    # OpenAI API
│   │   └── gemini.rs    # Gemini API (Google)
│   ├── strategy/
│   │   ├── mod.rs
│   │   ├── adaptive.rs  # AIMD algorithm implementation
│   │   └── metrics.rs   # EWMA and runtime metrics
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
    ├── prompt_enhancer_test.rs
    ├── third_party_api_test.rs
    ├── tools_test.rs
    └── utils_test.rs
```

## Adaptive Upload Strategy

The tool uses an AIMD (Additive Increase, Multiplicative Decrease) algorithm inspired by TCP congestion control to dynamically optimize upload performance:

### How It Works

1. **Warmup Phase**: Starts with concurrency=1, evaluates success rate over 5-10 requests, then jumps to target concurrency if successful
2. **Additive Increase**: When success rate > 95% and latency is healthy, concurrency increases by 1
3. **Multiplicative Decrease**: When success rate < 70%, rate limited, or high latency, concurrency halves and timeout increases by 50%

### Metrics

- **EWMA Latency**: Exponentially weighted moving average (α=0.2) for latency smoothing
- **Success Rate**: Calculated over a sliding window of 20 requests
- **Latency Health**: Compared against a fixed baseline to detect degradation

### Safety Bounds

| Parameter | Minimum | Maximum |
|-----------|---------|---------|
| Concurrency | 1 | 8 |
| Timeout | 15s | 180s |

### CLI Overrides

You can override individual parameters while keeping others adaptive:

```bash
# Fixed concurrency, adaptive timeout
ace-tool-rs --base-url ... --token ... --upload-concurrency 4

# Fixed timeout, adaptive concurrency
ace-tool-rs --base-url ... --token ... --upload-timeout 60

# Disable adaptive entirely (use static heuristic)
ace-tool-rs --base-url ... --token ... --no-adaptive
```

## Project Scale Strategies

The tool uses heuristic-based initial values based on project size. With adaptive mode enabled (default), these serve as target values that the AIMD algorithm works toward:

| Scale | Blob Count | Batch Size | Target Concurrency | Target Timeout |
|-------|------------|------------|-------------------|----------------|
| Small | < 100 | 10 | 1 | 30s |
| Medium | 100-499 | 30 | 2 | 45s |
| Large | 500-1999 | 50 | 3 | 60s |
| Extra Large | 2000+ | 70 | 4 | 90s |

With `--no-adaptive`, these values are used directly without runtime adjustment.

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

- **390+ unit tests** covering all major components
- Modular architecture with clear separation of concerns
- Async/await throughout using Tokio runtime
- Parallel file processing using Rayon
- Comprehensive error handling with `anyhow`

## Limitations

- Only processes the root `.gitignore` file (nested `.gitignore` files are not supported)
- Requires network access to the indexing API
- Maximum file size: 128KB per file
- Maximum batch size: 1MB per upload batch

## License

This project is dual-licensed:

### Non-Commercial / Personal Use - GNU General Public License v3.0

Free for personal projects, educational purposes, open source projects, and non-commercial use. See [LICENSE](LICENSE) for the full GPLv3 license text.

### Commercial / Workplace Use - Commercial License Required

**If you use ace-tool-rs in a commercial environment, workplace, or for any commercial purpose, you must obtain a commercial license.**

This includes but is not limited to:
- Using the software at work (any organization)
- Integrating into commercial products or services
- Using for client work or consulting
- Offering as part of a SaaS/cloud service

**Contact**: missdeer@gmail.com for commercial licensing inquiries.

See [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL) for more details.

## Author

[missdeer](https://github.com/missdeer)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
