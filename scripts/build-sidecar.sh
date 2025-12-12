#!/bin/bash
# Build claude-code-acp sidecar for macOS targets
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/.sidecar-build"
BINARIES_DIR="$PROJECT_ROOT/src-tauri/binaries"

echo "Building claude-code-acp sidecar..."

# Ensure bun is available (check common install locations)
if ! command -v bun &> /dev/null; then
    if [ -x "$HOME/.bun/bin/bun" ]; then
        export PATH="$HOME/.bun/bin:$PATH"
    else
        echo "Error: bun is required but not installed."
        echo "Install with: curl -fsSL https://bun.sh/install | bash"
        exit 1
    fi
fi

# Clean and create build directory
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
mkdir -p "$BINARIES_DIR"

cd "$BUILD_DIR"

# Initialize and install the package
echo "Installing @zed-industries/claude-code-acp..."
bun init -y > /dev/null
bun add @zed-industries/claude-code-acp

# Build for macOS ARM64 (Apple Silicon)
echo "Building for macOS ARM64..."
bun build ./node_modules/@zed-industries/claude-code-acp/dist/index.js \
    --compile \
    --minify \
    --target=bun-darwin-arm64 \
    --outfile "$BINARIES_DIR/claude-code-acp-aarch64-apple-darwin"

# Build for macOS x64 (Intel)
echo "Building for macOS x64..."
bun build ./node_modules/@zed-industries/claude-code-acp/dist/index.js \
    --compile \
    --minify \
    --target=bun-darwin-x64 \
    --outfile "$BINARIES_DIR/claude-code-acp-x86_64-apple-darwin"

# Cleanup
cd "$PROJECT_ROOT"
rm -rf "$BUILD_DIR"

echo "Sidecar binaries built successfully:"
ls -lh "$BINARIES_DIR"/claude-code-acp-*
