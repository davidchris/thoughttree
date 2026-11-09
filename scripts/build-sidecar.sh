#!/bin/bash
# Build claude-code-acp sidecar for specified or current platform
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
# The adapter version and its dependency tree are pinned in package.json and bun.lock.
ADAPTER_ENTRY="$PROJECT_ROOT/node_modules/@zed-industries/claude-code-acp/dist/index.js"
BINARIES_DIR="$PROJECT_ROOT/binaries"

# Target can be passed as argument: darwin-arm64, darwin-x64, linux-x64, windows-x64
TARGET="${1:-auto}"

build_target() {
    local bun_target="$1"
    local rust_triple="$2"
    local ext="${3:-}"

    echo "Building for $bun_target..."
    bun build "$ADAPTER_ENTRY" \
        --compile \
        --minify \
        --target="$bun_target" \
        --outfile "$BINARIES_DIR/claude-code-acp-${rust_triple}${ext}"
}

detect_platform() {
    local os=$(uname -s)
    local arch=$(uname -m)

    case "$os" in
        Darwin)
            case "$arch" in
                arm64) echo "darwin-arm64" ;;
                x86_64) echo "darwin-x64" ;;
                *) echo "unknown" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo "linux-x64" ;;
                aarch64) echo "linux-arm64" ;;
                *) echo "unknown" ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            echo "windows-x64"
            ;;
        *)
            echo "unknown"
            ;;
    esac
}

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

if [ ! -f "$ADAPTER_ENTRY" ]; then
    echo "Error: the pinned adapter is not installed. Run: bun install --frozen-lockfile"
    exit 1
fi
mkdir -p "$BINARIES_DIR"

# Auto-detect platform if not specified
if [ "$TARGET" = "auto" ]; then
    TARGET=$(detect_platform)
    if [ "$TARGET" = "unknown" ]; then
        echo "Error: Could not detect platform. Please specify target manually."
        echo "Usage: $0 [darwin-arm64|darwin-x64|linux-x64|windows-x64]"
        exit 1
    fi
    echo "Auto-detected platform: $TARGET"
fi

# Build for specified target
case "$TARGET" in
    darwin-arm64)
        build_target "bun-darwin-arm64" "aarch64-apple-darwin"
        ;;
    darwin-x64)
        build_target "bun-darwin-x64" "x86_64-apple-darwin"
        ;;
    linux-x64)
        build_target "bun-linux-x64" "x86_64-unknown-linux-gnu"
        ;;
    linux-arm64)
        build_target "bun-linux-arm64" "aarch64-unknown-linux-gnu"
        ;;
    windows-x64)
        build_target "bun-windows-x64" "x86_64-pc-windows-msvc" ".exe"
        ;;
    all-macos)
        # For local development on macOS - build both architectures
        build_target "bun-darwin-arm64" "aarch64-apple-darwin"
        build_target "bun-darwin-x64" "x86_64-apple-darwin"
        ;;
    *)
        echo "Error: Unknown target '$TARGET'"
        echo "Valid targets: darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64, all-macos"
        exit 1
        ;;
esac

echo "Sidecar binaries built successfully:"
ls -lh "$BINARIES_DIR"/claude-code-acp-* 2>/dev/null || echo "(no binaries found)"
