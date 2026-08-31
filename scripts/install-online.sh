#!/bin/sh
set -eu

REPO="bethropolis/podbox"
BINDIR="${PODBOX_BINDIR:-${HOME}/.local/bin}"
DOCS_URL="https://bethropolis.github.io/podbox/"

# ── cosmetics ────────────────────────────────────────────────────────────
# Respect NO_COLOR and non-tty output (this is piped through `sh`, so keep
# it plain whenever we can't be sure a real terminal is on the other end).
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD=$(printf '\033[1m');  DIM=$(printf '\033[2m')
    CYAN=$(printf '\033[36m'); GREEN=$(printf '\033[32m')
    YELLOW=$(printf '\033[33m'); RED=$(printf '\033[31m')
    RESET=$(printf '\033[0m')
else
    BOLD="" DIM="" CYAN="" GREEN="" YELLOW="" RED="" RESET=""
fi

step()  { printf "%s→%s %s\n" "$DIM" "$RESET" "$1"; }
ok()    { printf "%s✔%s %s\n" "$GREEN" "$RESET" "$1"; }
warn()  { printf "%s!%s %s\n" "$YELLOW" "$RESET" "$1"; }
die()   { printf "%s✘ %s%s\n" "$RED" "$1" "$RESET" >&2; exit 1; }
rule()  { i=0; line=""; while [ "$i" -lt "${1:-44}" ]; do line="${line}─"; i=$((i + 1)); done; printf "%s%s%s\n" "$DIM" "$line" "$RESET"; }

TMP=""
cleanup()     { [ -n "$TMP" ] && rm -rf "$TMP"; }
interrupted() { printf "\n"; die "Interrupted."; }
trap cleanup EXIT
trap interrupted INT TERM

printf "%s%s📦 podbox installer%s\n" "$BOLD" "$CYAN" "$RESET"
rule
echo

# ── environment checks ──────────────────────────────────────────────────
[ "$(uname -s)" = "Linux" ] || die "podbox only supports Linux (detected: $(uname -s))"

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)        ARCH="x86_64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)             die "Unsupported architecture: $ARCH (podbox ships linux/amd64 and linux/arm64 only)" ;;
esac

command -v curl >/dev/null 2>&1 || die "curl is required but not found"
command -v tar  >/dev/null 2>&1 || die "tar is required but not found"
command -v sha256sum >/dev/null 2>&1 && SHASUM=sha256sum || SHASUM=""
[ -n "$SHASUM" ] || warn "sha256sum not found — skipping checksum verification"

mkdir -p "$BINDIR" 2>/dev/null || die "Cannot create $BINDIR (check permissions, or set PODBOX_BINDIR)"
[ -w "$BINDIR" ] || die "$BINDIR is not writable (check permissions, or set PODBOX_BINDIR)"

# retry wrapper: network hiccups shouldn't sink the whole install
fetch() {
    url="$1"; out="$2"; tries=0
    while [ "$tries" -lt 3 ]; do
        if curl -sSfL --connect-timeout 10 "$url" -o "$out"; then
            return 0
        fi
        tries=$((tries + 1))
        [ "$tries" -lt 3 ] && { warn "Download attempt $tries/3 failed, retrying..."; sleep 2; }
    done
    return 1
}

# ── resolve version ──────────────────────────────────────────────────────
if [ -n "${PODBOX_VERSION:-}" ]; then
    TAG="$PODBOX_VERSION"
    case "$TAG" in v*) ;; *) TAG="v${TAG}" ;; esac
    step "Using pinned version ${BOLD}${TAG}${RESET}${DIM} (\$PODBOX_VERSION)${RESET}"
else
    step "Checking the latest release..."
    API_TMP=$(mktemp)
    HTTP_CODE=$(curl -sS -w '%{http_code}' -o "$API_TMP" \
        "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null) || HTTP_CODE="000"
    if [ "$HTTP_CODE" != "200" ]; then
        if grep -qi "rate limit" "$API_TMP" 2>/dev/null; then
            rm -f "$API_TMP"
            die "GitHub API rate limit hit. Try again later, or pin a version: PODBOX_VERSION=v0.7.1 sh -c \"\$(curl -fsSL ${DOCS_URL}install.sh)\""
        fi
        rm -f "$API_TMP"
        die "Failed to reach GitHub (HTTP ${HTTP_CODE}). Check your connection or https://github.com/${REPO}/releases"
    fi
    TAG=$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$API_TMP" | head -n1)
    rm -f "$API_TMP"
    [ -n "$TAG" ] || die "Could not parse the latest release tag from GitHub's response."
fi

# ── skip if already up to date ───────────────────────────────────────────
if [ -x "$BINDIR/podbox" ]; then
    CURRENT=$(  "$BINDIR/podbox" --version 2>/dev/null | tr -s ' ' | cut -d' ' -f2 || true)
    WANT=$(printf '%s' "$TAG" | sed 's/^v//')
    if [ -n "$CURRENT" ] && [ "$CURRENT" = "$WANT" ] && [ "${PODBOX_FORCE:-}" != "1" ]; then
        ok "podbox ${WANT} is already installed at ${BINDIR}/podbox"
        echo
        printf "%sReinstall anyway:%s PODBOX_FORCE=1 sh -c \"\$(curl -fsSL %sinstall.sh)\"\n" "$DIM" "$RESET" "$DOCS_URL"
        exit 0
    fi
    [ -n "$CURRENT" ] && step "Updating podbox ${CURRENT} → ${WANT}"
fi

echo
step "Downloading podbox ${BOLD}${TAG}${RESET}${DIM} for linux/${ARCH}...${RESET}"

TMP=$(mktemp -d)
cd "$TMP"

BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
ARCHIVE="podbox-${TAG}-linux-${ARCH}.tar.gz"

fetch "${BASE_URL}/${ARCHIVE}" "$ARCHIVE" \
    || die "Failed to download ${ARCHIVE} after 3 attempts. Check https://github.com/${REPO}/releases/tag/${TAG}"
fetch "${BASE_URL}/checksums.txt" "checksums.txt" \
    || die "Failed to download checksums.txt after 3 attempts."

if [ -n "$SHASUM" ]; then
    CHECKSUM_LINE=$(grep -F "$ARCHIVE" checksums.txt || true)
    # A missing match must be a hard failure — piping empty input straight to
    # `sha256sum -c` exits 0 with nothing checked, which would silently
    # "verify" an unverified file.
    [ -n "$CHECKSUM_LINE" ] || die "No checksum entry for ${ARCHIVE} in checksums.txt — refusing to install unverified."
    printf '%s\n' "$CHECKSUM_LINE" | sha256sum -c - >/dev/null 2>&1 \
        || die "Checksum verification FAILED for ${ARCHIVE}. The download may be corrupt or tampered with. Aborting."
    ok "Checksum verified"
fi

tar -tzf "$ARCHIVE" >/dev/null 2>&1 || die "Downloaded archive is not a valid tar.gz (try again)"

# ── extract just the binary ──────────────────────────────────────────────
# Extracting the whole archive straight into $BINDIR also dumps the release's
# LICENSE/README.md next to your binaries — extract to scratch space instead
# and pull out only the `podbox` executable, wherever it landed in the tree.
tar -xzf "$ARCHIVE"
BIN_PATH=$(find . -maxdepth 2 -type f -name podbox | head -n1)
[ -n "$BIN_PATH" ] || die "Could not find a 'podbox' binary inside the release archive."

NEW_BIN="${BINDIR}/podbox.new.$$"
mv "$BIN_PATH" "$NEW_BIN"
chmod +x "$NEW_BIN"

# Sanity-check before it becomes the live binary — a corrupt or
# wrong-architecture binary should never clobber a working install.
if ! "$NEW_BIN" --version >/dev/null 2>&1; then
    rm -f "$NEW_BIN"
    die "Downloaded binary failed to run (wrong arch or corrupt download). Nothing was changed."
fi
mv -f "$NEW_BIN" "$BINDIR/podbox"

# clean up strays from older/buggy installs
rm -f "$BINDIR/podbox-guest" "$BINDIR/LICENSE" "$BINDIR/README.md"

ok "Installed to ${BINDIR}/podbox"

# ── shell completions (best-effort; never fatal) ─────────────────────────
COMPLETIONS=""
if command -v bash >/dev/null 2>&1; then
    d="${XDG_DATA_HOME:-$HOME/.local/share}/bash-completion/completions"
    mkdir -p "$d" 2>/dev/null && "$BINDIR/podbox" completions bash > "$d/podbox" 2>/dev/null \
        && COMPLETIONS="${COMPLETIONS}bash "
fi
if command -v zsh >/dev/null 2>&1; then
    d="${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions"
    mkdir -p "$d" 2>/dev/null && "$BINDIR/podbox" completions zsh > "$d/_podbox" 2>/dev/null \
        && COMPLETIONS="${COMPLETIONS}zsh "
fi
if command -v fish >/dev/null 2>&1; then
    d="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions"
    mkdir -p "$d" 2>/dev/null && "$BINDIR/podbox" completions fish > "$d/podbox.fish" 2>/dev/null \
        && COMPLETIONS="${COMPLETIONS}fish "
fi
[ -n "$COMPLETIONS" ] && ok "Shell completions installed: ${COMPLETIONS}"

# ── done: guide the user forward ─────────────────────────────────────────
echo
rule
printf "%s%s✓ podbox %s installed%s\n" "$BOLD" "$GREEN" "$TAG" "$RESET"
rule
echo

case ":${PATH}:" in
    *:"${BINDIR}":*)
        printf "%sNext:%s\n" "$BOLD" "$RESET"
        printf "  1. %spodbox doctor%s          check your setup\n" "$CYAN" "$RESET"
        printf "  2. %spodbox create fedora%s   spin up your first container\n" "$CYAN" "$RESET"
        printf "  3. %spodbox enter%s           hop in\n" "$CYAN" "$RESET"
        ;;
    *)
        warn "${BINDIR} is not on your PATH yet."
        echo
        case "$(basename "${SHELL:-sh}")" in
            fish)
                RC="$HOME/.config/fish/config.fish"
                echo "  Add this to ${RC}:"
                printf "    %sfish_add_path %s%s\n" "$CYAN" "$BINDIR" "$RESET"
                ;;
            zsh)
                RC="$HOME/.zshrc"
                echo "  Add this to ${RC}:"
                printf "    %sexport PATH=\"%s:\$PATH\"%s\n" "$CYAN" "$BINDIR" "$RESET"
                ;;
            *)
                RC="$HOME/.bashrc"
                echo "  Add this to ${RC}:"
                printf "    %sexport PATH=\"%s:\$PATH\"%s\n" "$CYAN" "$BINDIR" "$RESET"
                ;;
        esac
        echo
        echo "  Then restart your shell (or run 'exec \$SHELL') and try:"
        printf "    %spodbox doctor%s\n" "$CYAN" "$RESET"
        ;;
esac

echo
printf "%sDocs:%s %s\n" "$DIM" "$RESET" "$DOCS_URL"
