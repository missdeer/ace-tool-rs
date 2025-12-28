#!/bin/bash

set -e

REPO="missdeer/ace-tool-rs"
INSTALL_DIR="/opt/ace-tool-rs"
BINARY_NAME="ace-tool-rs"

# Function to detect platform and architecture
detect_platform() {
	OS=$(uname -s)
	ARCH=$(uname -m)

	case "$OS" in
	Darwin)
		PLATFORM="Darwin"
		# macOS uses universal binary
		ASSET_ARCH="universal"
		EXTENSION="tar.gz"
		;;
	Linux)
		PLATFORM="Linux"
		case "$ARCH" in
		x86_64)
			ASSET_ARCH="x86_64"
			;;
		aarch64 | arm64)
			ASSET_ARCH="arm64"
			;;
		*)
			echo "Error: Unsupported architecture: $ARCH"
			exit 1
			;;
		esac
		EXTENSION="tar.gz"
		;;
	*)
		echo "Error: Unsupported operating system: $OS"
		exit 1
		;;
	esac
}

# Function to get latest version from GitHub API
get_latest_version() {
	local api_response=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest")
	if [ $? -ne 0 ] || [ -z "$api_response" ]; then
		echo "Error: Failed to get latest version from GitHub API" >&2
		exit 1
	fi

	local version=$(echo "$api_response" | grep '"tag_name":' | sed -E 's/.*"v([^"]+)".*/\1/')
	if [ -z "$version" ]; then
		echo "Error: Failed to parse version from GitHub API response" >&2
		exit 1
	fi
	echo "$version"
}

# Function to download and install binary from GitHub Release
download_from_github() {
	local version=$1
	local asset_name="ace-tool-rs_${PLATFORM}_${ASSET_ARCH}.${EXTENSION}"
	local download_url="https://github.com/${REPO}/releases/download/v${version}/${asset_name}"
	local temp_dir=$(mktemp -d)
	local archive_path="${temp_dir}/${asset_name}"

	echo "Downloading ace-tool-rs v${version} from GitHub Release..."
	echo "URL: ${download_url}"

	# Download the archive
	if ! curl -L -o "$archive_path" "$download_url"; then
		echo "Error: Failed to download binary from GitHub Release"
		rm -rf "$temp_dir"
		exit 1
	fi

	# Create install directory if it doesn't exist
	sudo mkdir -p "$INSTALL_DIR"

	# Extract archive
	echo "Extracting to ${INSTALL_DIR}..."
	cd "$temp_dir"
	if [ "$EXTENSION" = "tar.gz" ]; then
		tar -xzf "$archive_path"
	else
		echo "Error: Unsupported archive format: $EXTENSION"
		rm -rf "$temp_dir"
		exit 1
	fi

	# Move binary to install directory
	if [ -f "$BINARY_NAME" ]; then
		sudo mv "$BINARY_NAME" "${INSTALL_DIR}/${BINARY_NAME}"
		sudo chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
		echo "Binary installed to ${INSTALL_DIR}/${BINARY_NAME}"
	else
		echo "Error: Binary not found in archive"
		rm -rf "$temp_dir"
		exit 1
	fi

	# Cleanup
	rm -rf "$temp_dir"

	echo "${INSTALL_DIR}/${BINARY_NAME}"
}

# check the first argument is the path to the ace-tool-rs binary
if [ -n "$1" ]; then
	ACE_TOOL_RS_PATH="$1"
fi

if [ -z "$ACE_TOOL_RS_PATH" ]; then
	# Get the absolute path of the ace-tool-rs binary
	# if current os is Darwin, use $(pwd)/ace-tool-rs
	if [ "$(uname)" == "Darwin" ]; then
		ACE_TOOL_RS_PATH=$(pwd)/ace-tool-rs
	fi
	if [ ! -f "$ACE_TOOL_RS_PATH" ]; then
		ACE_TOOL_RS_PATH=$(pwd)/target/release/ace-tool-rs
		if [ ! -f "$ACE_TOOL_RS_PATH" ]; then
			# Check if binary exists in /opt directory
			if [ -f "${INSTALL_DIR}/${BINARY_NAME}" ]; then
				ACE_TOOL_RS_PATH="${INSTALL_DIR}/${BINARY_NAME}"
			else
				# Download from GitHub Release
				echo "ace-tool-rs binary not found locally, downloading from GitHub Release..."
				detect_platform
				VERSION=$(get_latest_version) || exit 1
				ACE_TOOL_RS_PATH=$(download_from_github "$VERSION") || exit 1
			fi
		fi
	fi
fi

# Add the ace-tool-rs server to the Claude Code MCP registry
CLAUDE_PATH=$(which claude)
if [ -f "$CLAUDE_PATH" ]; then
	"$CLAUDE_PATH" mcp add-json ace-tool -s user '{"type":"stdio","command":"ace-tool-rs","args":["--base-url",  "https://api.example.com/",  "--token", "ace_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"],"env":{}}'
else
	echo "Error: claude not found"
	exit 1
fi
