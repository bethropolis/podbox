#!/usr/bin/env bash
# Build the podbox-wasm Studio bindings and drop the artifacts into the
# website, which are committed to git (deterministic deploys, works offline).
set -euo pipefail

# ── colour ────────────────────────────────────────────────────────────────
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  BOLD=$'\e[1m' CYAN=$'\e[36m' GREEN=$'\e[32m' YELLOW=$'\e[33m' RED=$'\e[31m' RST=$'\e[0m'
else
  BOLD='' CYAN='' GREEN='' YELLOW='' RED='' RST=''
fi
step() { printf '  %s==>%s %s%s%s\n' "$CYAN" "$RST" "$BOLD" "$1" "$RST"; }
ok() { printf '  %s✔%s %s\n' "$GREEN" "$RST" "$1"; }
warn() { printf '  %s!%s %s\n' "$YELLOW" "$RST" "$1"; }
die() {
  printf '  %s✗%s %s\n' "$RED" "$RST" "$1" >&2
  exit 1
}

# ── locate wasm-pack (PATH first, then mise installs) ─────────────────────
# Candidates are verified with `--version`: a mise shim with no activated
# version resolves on PATH but fails to run, so it must be skipped.
find_wasm_pack() {
  local candidate
  candidate="$(command -v wasm-pack 2>/dev/null)" || candidate=""
  if [[ -n "$candidate" ]] && "$candidate" --version &>/dev/null; then
    printf '%s' "$candidate"
    return 0
  fi
  local mise_bin
  mise_bin="$(ls -d "${MISE_DATA_DIR:-$HOME/.local/share/mise}"/installs/*wasm-pack/*/wasm-pack 2>/dev/null | sort -V | tail -n 1)"
  if [[ -n "$mise_bin" ]] && "$mise_bin" --version &>/dev/null; then
    printf '%s' "$mise_bin"
    return 0
  fi
  return 1
}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$REPO_ROOT/website/src/wasm"

step "Locating wasm-pack"
WASM_PACK="$(find_wasm_pack)" || die "wasm-pack not found (cargo install wasm-pack, or: mise install github:wasm-bindgen/wasm-pack)"
ok "using $WASM_PACK ($("$WASM_PACK" --version))"

step "Building podbox-wasm for wasm32-unknown-unknown"
"$WASM_PACK" build "$REPO_ROOT/crates/podbox-wasm" \
  --target web \
  --out-dir "$OUT_DIR" \
  --release

# Build artifacts the website doesn't need.
rm -f "$OUT_DIR/.gitignore" "$OUT_DIR/package.json"

# Optimize WASM size when binaryen is available (optional).
if command -v wasm-opt &>/dev/null; then
  step "Optimizing with wasm-opt"
  wasm-opt -O3 "$OUT_DIR/podbox_wasm_bg.wasm" -o "$OUT_DIR/podbox_wasm_bg.wasm"
else
  warn "wasm-opt not found, skipping size optimization"
fi

ok "WASM build complete in website/src/wasm"
ls -la "$OUT_DIR"
