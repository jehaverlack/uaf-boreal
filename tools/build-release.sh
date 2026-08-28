#!/usr/bin/env bash
set -euo pipefail

APP="boreal"
OUT="dist"

LINUX_X86_64="x86_64-unknown-linux-gnu"
LINUX_AARCH64="aarch64-unknown-linux-gnu"
LINUX_ARMV7="armv7-unknown-linux-gnueabihf"
WINDOWS_X86_64="x86_64-pc-windows-gnu"

check_target() {
    local target="$1"

    if ! rustup target list --installed | grep -qx "$target"; then
        echo "ERROR: Rust target not installed: $target"
        echo "Install with:"
        echo "  rustup target add $target"
        exit 1
    fi
}

check_command() {
    local command="$1"
    local hint="$2"

    if ! command -v "$command" >/dev/null 2>&1; then
        echo "ERROR: Required command not found: $command"
        echo "$hint"
        exit 1
    fi
}

echo "==> BOREAL release build"

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "ERROR: This build script is currently intended for Linux."
    exit 1
fi

echo "==> Checking Rust targets"

check_target "$LINUX_X86_64"
check_target "$LINUX_AARCH64"
check_target "$LINUX_ARMV7"
check_target "$WINDOWS_X86_64"

echo "==> Checking cross-compilers"

check_command \
    aarch64-linux-gnu-gcc \
    "Install with: sudo apt install gcc-aarch64-linux-gnu"

check_command \
    arm-linux-gnueabihf-gcc \
    "Install with: sudo apt install gcc-arm-linux-gnueabihf"

check_command \
    x86_64-w64-mingw32-gcc \
    "Install with: sudo apt install gcc-mingw-w64-x86-64"

mkdir -p "$OUT"

echo
echo "==> Building Linux x86_64"

cargo build \
    --release \
    --target "$LINUX_X86_64"

cp \
    "target/$LINUX_X86_64/release/$APP" \
    "$OUT/boreal-linux-x86_64"

echo
echo "==> Building Linux ARM64"

cargo build \
    --release \
    --target "$LINUX_AARCH64"

cp \
    "target/$LINUX_AARCH64/release/$APP" \
    "$OUT/boreal-linux-aarch64"

echo
echo "==> Building Linux ARMv7"

cargo build \
    --release \
    --target "$LINUX_ARMV7"

cp \
    "target/$LINUX_ARMV7/release/$APP" \
    "$OUT/boreal-linux-armv7"

echo
echo "==> Building Windows x86_64"

cargo build \
    --release \
    --target "$WINDOWS_X86_64"

cp \
    "target/$WINDOWS_X86_64/release/$APP.exe" \
    "$OUT/boreal-windows-x86_64.exe"

echo
echo "==> macOS builds skipped"
echo "    macOS binaries require an Apple SDK/toolchain."
echo "    Build these on a macOS host or CI runner:"
echo "      x86_64-apple-darwin"
echo "      aarch64-apple-darwin"

echo
echo "==> Build complete"
ls -lh "$OUT"