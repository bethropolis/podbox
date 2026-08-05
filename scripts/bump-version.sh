#!/usr/bin/env bash
# Bump the workspace version in Cargo.toml, commit, and tag.
# Usage: scripts/bump-version.sh <new-version>
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "  e.g. $0 0.3.6"
    exit 1
fi

NEW_VERSION="$1"

# Validate version format (semver with optional pre-release suffix)
if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
    echo "Error: version must be MAJOR.MINOR.PATCH (e.g. 0.3.6)"
    echo "  or MAJOR.MINOR.PATCH-PRE (e.g. 0.6.0-pre.1)"
    exit 1
fi

# Warn if not on main, but allow (useful for staging pre-releases)
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo "Warning: not on main branch (currently on '$CURRENT_BRANCH')"
fi

if ! git diff --quiet; then
    echo "Error: working tree is not clean. Commit or stash first."
    exit 1
fi

# Update workspace Cargo.toml
sed -i -E "s/^version[[:space:]]*=[[:space:]]*\".*\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update individual crate versions if they use workspace version
# (they inherit from workspace.package, so only the workspace Cargo.toml needs updating)

echo "Cargo.toml version updated to $NEW_VERSION"

# Commit
git add Cargo.toml
git commit -m "chore: bump version to $NEW_VERSION"

# Tag
git tag -a "v$NEW_VERSION" -m "v$NEW_VERSION"

echo ""
echo "Committed and tagged v$NEW_VERSION"
echo "Push with: git push && git push --tags"
