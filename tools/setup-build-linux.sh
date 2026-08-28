#!/usr/bin/env bash
set -euo pipefail

TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "armv7-unknown-linux-gnueabihf"
    "x86_64-pc-windows-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
)

echo "==> BOREAL Linux build environment setup"

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "ERROR: This script is intended for Linux."
    exit 1
fi

if ! command -v apt >/dev/null 2>&1; then
    echo "ERROR: This setup script currently supports Debian/Ubuntu systems using apt."
    exit 1
fi

echo "==> Installing system build dependencies"

sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    curl \
    gcc-mingw-w64-x86-64 \
    gcc-aarch64-linux-gnu \
    gcc-arm-linux-gnueabihf

if ! command -v rustup >/dev/null 2>&1; then
    echo "==> Installing Rust using rustup"

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck disable=SC1090
        source "$HOME/.cargo/env"
    fi
else
    echo "==> rustup already installed"
fi

if ! command -v cargo >/dev/null 2>&1; then
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck disable=SC1090
        source "$HOME/.cargo/env"
    fi
fi

echo "==> Checking Rust targets"

for target in "${TARGETS[@]}"; do
    echo "==> Checking Rust target: $target"

    if rustup target list --installed | grep -qx "$target"; then
        echo "Rust target already installed: $target"
    else
        rustup target add "$target"
    fi
done

echo
echo "==> Build environment ready"
echo

rustc --version
cargo --version
rustup --version

echo
echo "Installed Rust targets:"
rustup target list --installed