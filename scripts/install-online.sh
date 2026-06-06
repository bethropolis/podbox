#!/bin/sh
set -eu

REPO="bethropolis/podbox"
BINDIR="${HOME}/.local/bin"

# architecture detection
OS="linux"
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)         ARCH="x86_64" ;;
    aarch64|arm64)  ARCH="arm64" ;;
    *)
        echo "Unsupported architecture: $ARCH"
        echo "podbox is available for linux/amd64 and linux/arm64."
        exit 1
        ;;
esac

command -v curl >/dev/null 2>&1 || { echo "curl is required"; exit 1; }
command -v sha256sum >/dev/null 2>&1 && SHASUM=sha256sum || SHASUM=""

echo "Fetching latest podbox release..."

LATEST=$(curl -sSfL "https://api.github.com/repos/${REPO}/releases/latest")
TAG=$(echo "$LATEST" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')

if [ -z "$TAG" ]; then
    echo "Failed to detect latest release."
    echo "Check https://github.com/${REPO}/releases"
    exit 1
fi

echo "Downloading podbox ${TAG} for ${OS}/${ARCH}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
cd "$TMP"

BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

curl -sSfLO "${BASE_URL}/podbox-${TAG}-${OS}-${ARCH}.tar.gz"
curl -sSfLO "${BASE_URL}/checksums.txt"

# verify checksums
if [ -n "$SHASUM" ]; then
    grep -E "(podbox-${TAG}-${OS}-${ARCH})" checksums.txt \
        | sha256sum -c - 2>/dev/null || {
            echo "Checksum verification failed. Aborting."
            exit 1
        }
    echo "Checksums verified."
fi

# install
mkdir -p "$BINDIR"
tar -xzf "podbox-${TAG}-${OS}-${ARCH}.tar.gz" -C "$BINDIR"
chmod +x "$BINDIR/podbox"

echo "Installed podbox ${TAG} to ${BINDIR}  (podbox-guest embedded)"

# shell completions
if command -v "$BINDIR/podbox" >/dev/null 2>&1; then
    comp_dir="${XDG_DATA_HOME:-$HOME/.local/share}/completions"
    mkdir -p "$comp_dir" 2>/dev/null || true
    "$BINDIR/podbox" completions bash > "$comp_dir/podbox.bash" 2>/dev/null || true
fi

# PATH hint
case ":${PATH}:" in
    *:"${BINDIR}":*) ;;
    *)
        echo ""
        echo "  ${BINDIR} is not in your PATH. Add this to your shell rc:"
        echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
        ;;
esac
