#!/bin/sh
# Check current CPU type
CPU_TYPE=$(uname -m)

if [ "$CPU_TYPE" = "arm64" ]; then
	# Apple Silicon: build for x86_64
	TARGET="x86_64-apple-darwin"
else
	# Intel: build for aarch64
	TARGET="aarch64-apple-darwin"
fi

env RUSTFLAGS="-C target-cpu=native" cargo build --release
cargo build --release --target $TARGET
lipo -create -output ace-tool-rs target/$TARGET/release/ace-tool-rs target/release/ace-tool-rs
