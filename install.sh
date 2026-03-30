#!/usr/bin/env bash
# Cordon install script
# Usage: bash install.sh
# Builds a release binary and installs it to ~/.local/bin/cordon

set -euo pipefail

CYAN='\033[1;96m'; GREEN='\033[1;32m'; RED='\033[1;31m'; RESET='\033[0m'; BOLD='\033[1m'

echo -e "${CYAN}${BOLD}Cordon — Install Script${RESET}"
echo

# ── Pre-flight checks ─────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo -e "${RED}Error: Rust/cargo not found. Install via: https://rustup.rs${RESET}"
    exit 1
fi

if ! command -v bwrap &>/dev/null; then
    echo -e "${RED}Warning: bubblewrap (bwrap) not found.${RESET}"
    echo "  Debian/Ubuntu: sudo apt install bubblewrap"
    echo "  Fedora/RHEL:   sudo dnf install bubblewrap"
    echo "  Arch:          sudo pacman -S bubblewrap"
    echo
fi

# ── Build ─────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo -e "${CYAN}⟳ Building release binary…${RESET}"
cargo build --release --manifest-path="$SCRIPT_DIR/Cargo.toml"
echo -e "${GREEN}  Build OK${RESET}"

# ── Install ───────────────────────────────────────────────────────────────────
INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/target/release/cordon" "$INSTALL_DIR/cordon"
chmod +x "$INSTALL_DIR/cordon"

echo
echo -e "${GREEN}${BOLD}✅ Installed: $INSTALL_DIR/cordon${RESET}"
echo

# ── PATH reminder ─────────────────────────────────────────────────────────────
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo "  Add to your shell config:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo
fi

echo -e "${CYAN}Quick start:${RESET}"
echo "  cordon scan                           # first-time system scan (~30s)"
echo "  cordon run -- echo 'hello sandbox'   # run any command sandboxed"
echo "  cordon run --net=allow -- npm install # safe npm install"
echo
echo -e "${CYAN}Docs:${RESET} https://github.com/LORDv1shnu/Cordon"
