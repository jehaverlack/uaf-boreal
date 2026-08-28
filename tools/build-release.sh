#!/usr/bin/env bash
set -euo pipefail

# -------------------------------------------------
# Resolve project root
# -------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# -------------------------------------------------
# Application
# -------------------------------------------------

APP="boreal"
OUT="build"

# -------------------------------------------------
# Rust targets
# -------------------------------------------------

LINUX_X86_64="x86_64-unknown-linux-gnu"
LINUX_AARCH64="aarch64-unknown-linux-gnu"
LINUX_ARMV7="armv7-unknown-linux-gnueabihf"

WINDOWS_X86_64="x86_64-pc-windows-gnu"

DARWIN_X86_64="x86_64-apple-darwin"
DARWIN_AARCH64="aarch64-apple-darwin"

# -------------------------------------------------
# Helpers
# -------------------------------------------------

check_target() {
    local target="$1"

    if ! rustup target list --installed | grep -qx "$target"; then
        echo "ERROR: Rust target not installed: $target"
        echo
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
        echo
        echo "$hint"
        exit 1
    fi
}

# -------------------------------------------------
# Prerequisites
# -------------------------------------------------

echo "==> BOREAL release build"

check_command jq "Install jq before building."

VERSION="$(jq -r '.METADATA.version' metadata.json)"

if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
    echo "ERROR: Unable to read METADATA.version from metadata.json"
    exit 1
fi

HOST_OS="$(uname -s)"

echo "==> Version: v${VERSION}"
echo "==> Build host: ${HOST_OS}"

mkdir -p "$OUT"

# -------------------------------------------------
# Linux build host
# -------------------------------------------------

if [[ "$HOST_OS" == "Linux" ]]; then

    echo
    echo "==> Checking Linux/Windows Rust targets"

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

    # ---------------------------------------------
    # Linux x86_64
    # ---------------------------------------------

    echo
    echo "==> Building Linux x86_64"

    cargo build \
        --release \
        --target "$LINUX_X86_64"

    cp \
        "target/$LINUX_X86_64/release/$APP" \
        "$OUT/${APP}-v${VERSION}-linux-x86_64"

    # ---------------------------------------------
    # Linux ARM64
    # ---------------------------------------------

    echo
    echo "==> Building Linux ARM64"

    cargo build \
        --release \
        --target "$LINUX_AARCH64"

    cp \
        "target/$LINUX_AARCH64/release/$APP" \
        "$OUT/${APP}-v${VERSION}-linux-aarch64"

    # ---------------------------------------------
    # Linux ARMv7
    # ---------------------------------------------

    echo
    echo "==> Building Linux ARMv7"

    cargo build \
        --release \
        --target "$LINUX_ARMV7"

    cp \
        "target/$LINUX_ARMV7/release/$APP" \
        "$OUT/${APP}-v${VERSION}-linux-armv7"

    # ---------------------------------------------
    # Windows x86_64
    # ---------------------------------------------

    echo
    echo "==> Building Windows x86_64"

    cargo build \
        --release \
        --target "$WINDOWS_X86_64"

    cp \
        "target/$WINDOWS_X86_64/release/$APP.exe" \
        "$OUT/${APP}-v${VERSION}-windows-x86_64.exe"

    # ---------------------------------------------
    # Darwin notice
    # ---------------------------------------------

    echo
    echo "==> macOS builds not available from standard Linux toolchain"
    echo "    Build these targets on a macOS host:"
    echo "      $DARWIN_X86_64"
    echo "      $DARWIN_AARCH64"

# -------------------------------------------------
# macOS build host
# -------------------------------------------------

elif [[ "$HOST_OS" == "Darwin" ]]; then

    echo
    echo "==> Checking macOS Rust targets"

    check_target "$DARWIN_X86_64"
    check_target "$DARWIN_AARCH64"

    # ---------------------------------------------
    # macOS Intel
    # ---------------------------------------------

    echo
    echo "==> Building macOS x86_64"

    cargo build \
        --release \
        --target "$DARWIN_X86_64"

    cp \
        "target/$DARWIN_X86_64/release/$APP" \
        "$OUT/${APP}-v${VERSION}-macos-x86_64"

    # ---------------------------------------------
    # macOS Apple Silicon
    # ---------------------------------------------

    echo
    echo "==> Building macOS ARM64"

    cargo build \
        --release \
        --target "$DARWIN_AARCH64"

    cp \
        "target/$DARWIN_AARCH64/release/$APP" \
        "$OUT/${APP}-v${VERSION}-macos-aarch64"

else
    echo "ERROR: Unsupported build host: $HOST_OS"
    exit 1
fi

# -------------------------------------------------
# Results
# -------------------------------------------------

echo
echo "==> Build complete"
echo "==> Version: v${VERSION}"
echo
ls -lh "$OUT"