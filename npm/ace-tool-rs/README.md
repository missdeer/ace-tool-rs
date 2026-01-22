# ace-tool-rs

MCP server for codebase indexing, semantic search, and prompt enhancement.

## Installation

```bash
# Install globally
npm install -g ace-tool-rs

# Or run directly with npx
npx ace-tool-rs --help
```

## How It Works

This package uses platform-specific optional dependencies to provide pre-built binaries. When you install `ace-tool-rs`, npm automatically downloads the correct binary for your platform.

### Supported Platforms

| Platform | Architecture | Package |
|----------|--------------|---------|
| macOS    | x64, ARM64   | `@ace-tool-rs/darwin-universal` |
| Linux    | x64          | `@ace-tool-rs/linux-x64` |
| Linux    | ARM64        | `@ace-tool-rs/linux-arm64` |
| Windows  | x64          | `@ace-tool-rs/win32-x64` |
| Windows  | ARM64        | `@ace-tool-rs/win32-arm64` |

## Usage

```bash
ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

## Troubleshooting

### Binary not found

If the platform-specific package failed to install, you can install it manually:

```bash
# For Linux x64
npm install @ace-tool-rs/linux-x64

# For macOS
npm install @ace-tool-rs/darwin-universal

# For Windows x64
npm install @ace-tool-rs/win32-x64
```

### Alternative installation

If you have Rust installed, you can build from source:

```bash
cargo install ace-tool-rs
```

## License

GPL-3.0-only

For commercial use, please contact missdeer@gmail.com for licensing options.

## Verifying Downloads

Each GitHub release includes a `SHA256SUMS` file for integrity verification:

```bash
# Download the binary and checksum file
curl -LO https://github.com/missdeer/ace-tool-rs/releases/latest/download/ace-tool-rs_Linux_x86_64.tar.gz
curl -LO https://github.com/missdeer/ace-tool-rs/releases/latest/download/SHA256SUMS

# Verify the checksum
sha256sum -c SHA256SUMS --ignore-missing
```
