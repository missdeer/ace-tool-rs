# ace-tool-rs npm shim

This npm package provides a convenient way to install and run `ace-tool-rs` via npm/npx.

## How It Works

This is a **shim package** that automatically downloads the appropriate pre-built binary for your platform from GitHub Releases when first run. The binary is cached locally for subsequent invocations.

## Quick Start

```bash
# Run directly with npx (no installation needed)
npx ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>

# Or install globally
npm install -g ace-tool-rs
ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

## Supported Platforms

| Platform | Architecture | Status |
|----------|--------------|--------|
| Windows  | x64, ARM64   | Supported |
| macOS    | x64, ARM64   | Supported (universal binary) |
| Linux    | x64, ARM64   | Supported |

## Cache Location

Downloaded binaries are cached in platform-specific directories:

| Platform | Cache Path |
|----------|------------|
| Windows  | `%LOCALAPPDATA%\ace-tool-rs\<version>\` |
| macOS    | `~/Library/Caches/ace-tool-rs/<version>/` |
| Linux    | `$XDG_CACHE_HOME/ace-tool-rs/<version>/` or `~/.cache/ace-tool-rs/<version>/` |

The cache is versioned, so upgrading the npm package will download a new binary matching that version.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `GITHUB_TOKEN` | GitHub personal access token to avoid rate limits when downloading |

## Requirements

- Node.js 14.14.0 or later
- `tar` command (Linux/macOS) or PowerShell 5.0+ (Windows) for extraction

## Troubleshooting

### Download Fails

If automatic download fails, you can:

1. **Manual download**: Download the appropriate binary from [GitHub Releases](https://github.com/missdeer/ace-tool-rs/releases) and place it in the cache directory.

2. **Install via Cargo**: If you have Rust installed:
   ```bash
   cargo install ace-tool-rs
   ```

3. **Set GITHUB_TOKEN**: If you're hitting GitHub API rate limits:
   ```bash
   export GITHUB_TOKEN=your_github_token
   npx ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
   ```

### Binary Not Found After Extraction

Ensure your system has the required extraction tools:
- **Windows**: PowerShell 5.0+ with `Expand-Archive` cmdlet
- **Linux/macOS**: `tar` command

## How the Shim Works

1. On first run, checks if the binary exists in the cache directory
2. If not found, queries GitHub API for the release matching the npm package version
3. Downloads the platform-appropriate archive (`.zip` for Windows, `.tar.gz` for others)
4. Extracts the binary to the cache directory
5. Executes the binary with all provided arguments

## License

This package is part of [ace-tool-rs](https://github.com/missdeer/ace-tool-rs) and is licensed under GPL-3.0.

For commercial use, please contact missdeer@gmail.com for licensing options.
