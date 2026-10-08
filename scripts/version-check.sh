#!/usr/bin/env bash
# Pre-push check: warn if Cargo.toml version doesn't match the latest git tag.
# Warning only — never blocks. Runs via the hk `pre-push` hook (see hk.pkl).
set -euo pipefail

LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
TAG_VERSION="${LATEST_TAG#v}"

CARGO_VERSION=$(awk -F '"' '/^version[[:space:]]*=/{print $2; exit}' Cargo.toml)

if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
    echo ""
    echo "  Warning: Cargo.toml version ($CARGO_VERSION) differs from latest tag ($LATEST_TAG)"
    echo "  Run: scripts/bump-version.sh $TAG_VERSION"
    echo "  Or:  scripts/bump-version.sh <new-version>  (to create a new tag)"
    echo ""
fi
