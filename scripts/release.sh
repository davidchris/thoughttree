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

echo "Updating version files to $VERSION..."

# Update package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" package.json
echo "  Updated package.json"

# Update the GPUI crate, whose version the app bundle reports
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" crates/thoughttree-gpui/Cargo.toml
echo "  Updated crates/thoughttree-gpui/Cargo.toml"

# Update the workspace entries in Cargo.lock
echo "  Updating Cargo.lock..."
cargo update --workspace --quiet

# main accepts only squash-merged PRs, so a local commit or tag never reaches it.
echo ""
echo "Version files updated to $VERSION. Nothing was committed or tagged."
echo ""
echo "Next steps:"
echo "  1. Commit the changes on a branch and open a PR."
echo "  2. Merge the PR."
echo "  3. Tag the squash commit on main and push the tag:"
echo "       git fetch origin"
echo "       git tag v$VERSION <squash commit>"
echo "       git push origin v$VERSION"
