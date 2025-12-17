#!/bin/bash
set -e

VERSION=$1

if [ -z "$VERSION" ]; then
  echo "Usage: ./scripts/release.sh <version>"
  echo "Example: ./scripts/release.sh 0.2.0"
  exit 1
fi

# Validate semver format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Version must be in semver format (e.g., 0.2.0)"
  exit 1
fi

echo "Releasing v$VERSION..."

# Update package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" package.json
echo "  Updated package.json"

# Update tauri.conf.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" src-tauri/tauri.conf.json
echo "  Updated src-tauri/tauri.conf.json"

# Update Cargo.toml
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" src-tauri/Cargo.toml
echo "  Updated src-tauri/Cargo.toml"

# Update Cargo.lock
echo "  Updating Cargo.lock..."
(cd src-tauri && cargo check --quiet)

# Commit and tag
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: release v$VERSION"
git tag "v$VERSION"

echo ""
echo "Release v$VERSION created!"
echo ""
echo "Next steps:"
echo "  git push && git push --tags"
echo "  bun tauri build"
