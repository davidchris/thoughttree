#!/usr/bin/env bash
# Build the native desktop executable and a local macOS application bundle.
set -euo pipefail

GPUI_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GPUI_PROFILE=release
GPUI_BUILD=true
for argument in "$@"; do
    case "$argument" in
        --debug) GPUI_PROFILE=debug ;;
        --no-build) GPUI_BUILD=false ;;
        --help|-h)
            echo "Usage: scripts/build-gpui.sh [--debug] [--no-build]"
            echo "--no-build packages an existing executable in the selected profile."
            exit 0 ;;
        *) echo "Unknown option: $argument" >&2; exit 2 ;;
    esac
done

cd "$GPUI_ROOT"
GPUI_TARGET="${CARGO_TARGET_DIR:-$GPUI_ROOT/target}"
if [[ "$GPUI_TARGET" != /* ]]; then GPUI_TARGET="$GPUI_ROOT/$GPUI_TARGET"; fi
if $GPUI_BUILD; then
    if [[ "$GPUI_PROFILE" == release ]]; then
        cargo build --locked -p thoughttree-gpui --release
    else
        cargo build --locked -p thoughttree-gpui
    fi
fi
GPUI_BINARY="$GPUI_TARGET/$GPUI_PROFILE/thoughttree-gpui"
if [[ ! -x "$GPUI_BINARY" ]]; then
    echo "Native executable not found: $GPUI_BINARY" >&2
    exit 1
fi

GPUI_HOST="$(rustc -vV | sed -n 's/^host: //p')"
GPUI_SIDECAR="$GPUI_ROOT/src-tauri/binaries/claude-code-acp-$GPUI_HOST"
if [[ "$(uname -s)" != Darwin ]]; then
    if [[ -x "$GPUI_SIDECAR" ]]; then
        cp "$GPUI_SIDECAR" "$GPUI_TARGET/$GPUI_PROFILE/claude-code-acp"
    fi
    echo "$GPUI_BINARY"
    exit 0
fi

GPUI_BUNDLE="$GPUI_TARGET/$GPUI_PROFILE/bundle/ThoughtTree GPUI.app"
mkdir -p "$GPUI_BUNDLE/Contents/MacOS" "$GPUI_BUNDLE/Contents/Resources"
cp "$GPUI_BINARY" "$GPUI_BUNDLE/Contents/MacOS/thoughttree-gpui"
cp "$GPUI_ROOT/src-tauri/icons/icon.icns" "$GPUI_BUNDLE/Contents/Resources/icon.icns"
if [[ -x "$GPUI_SIDECAR" ]]; then
    cp "$GPUI_SIDECAR" "$GPUI_BUNDLE/Contents/MacOS/claude-code-acp"
else
    echo "Claude sidecar absent. Run bun run build:sidecar, then package again to include it." >&2
fi
cat > "$GPUI_BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>ThoughtTree GPUI</string>
<key>CFBundleDisplayName</key><string>ThoughtTree GPUI</string>
<key>CFBundleIdentifier</key><string>com.david.thoughttree.gpui</string>
<key>CFBundleExecutable</key><string>thoughttree-gpui</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.5.0</string>
<key>CFBundleVersion</key><string>0.5.0</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
plutil -lint "$GPUI_BUNDLE/Contents/Info.plist" > /dev/null
# Local development identity only. This does not notarize a release.
codesign --force --deep --sign - "$GPUI_BUNDLE"
echo "$GPUI_BUNDLE"
