#!/usr/bin/env bash
# Package the release application bundle as the macOS arm64 release DMG.
set -euo pipefail

DMG_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DMG_TAG=""
while (($#)); do
    case "$1" in
        --tag)
            [[ $# -ge 2 && -n "$2" ]] || { echo "--tag needs a value" >&2; exit 2; }
            DMG_TAG="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: scripts/build-dmg.sh [--tag v<version>]"
            echo "Run scripts/build-gpui.sh first. --tag fails before packaging when the tag differs from the bundle version."
            exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done

DMG_TARGET="${CARGO_TARGET_DIR:-$DMG_ROOT/target}"
if [[ "$DMG_TARGET" != /* ]]; then DMG_TARGET="$DMG_ROOT/$DMG_TARGET"; fi
DMG_APP="$DMG_TARGET/release/bundle/ThoughtTree.app"
DMG_VERSION="$(plutil -extract CFBundleShortVersionString raw -o - "$DMG_APP/Contents/Info.plist")"
if [[ -n "$DMG_TAG" && "${DMG_TAG#v}" != "$DMG_VERSION" ]]; then
    echo "Tag $DMG_TAG does not match bundle version $DMG_VERSION." >&2
    exit 1
fi

DMG_FILE="$DMG_TARGET/release/bundle/ThoughtTree_${DMG_VERSION}_aarch64.dmg"
DMG_STAGE="$(mktemp -d)"
trap 'rm -rf "$DMG_STAGE"' EXIT
chmod 755 "$DMG_STAGE"
ditto "$DMG_APP" "$DMG_STAGE/ThoughtTree.app"
ln -s /Applications "$DMG_STAGE/Applications"
hdiutil create -volname ThoughtTree -srcfolder "$DMG_STAGE" -fs HFS+ -format UDZO -ov "$DMG_FILE" > /dev/null
echo "$DMG_FILE"
