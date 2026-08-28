#!/usr/bin/env bash
set -euo pipefail

TARGETS=(
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
)

echo "==> BOREAL macOS build environment setup"

#
# This script configures a non-root user's BOREAL build
# environment on macOS.
#
# Rust is installed and managed per-user under:
#
#     ~/.cargo
#     ~/.rustup
#
# Any existing system-wide Rust installation is left
# unchanged.
#

if [[ "${EUID}" -eq 0 ]]; then
    echo "ERROR: Do not run this script as root."
    echo
    echo "Run it as the user who will build BOREAL:"
    echo
    echo "    ./tools/setup-build-macos.sh"
    echo
    exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: This script is intended for macOS."
    exit 1
fi

#
# Determine the user's shell configuration file.
#
# Modern macOS normally uses zsh.
#
case "${SHELL:-}" in
    */zsh)
        SHELL_RC="${HOME}/.zshrc"
        ;;
    */bash)
        SHELL_RC="${HOME}/.bashrc"
        ;;
    *)
        SHELL_RC="${HOME}/.zshrc"
        ;;
esac

#
# BOREAL uses a per-user Rust environment.
#
# These intentionally override any system-wide Rust
# configuration inherited from the shell.
#
export CARGO_HOME="${HOME}/.cargo"
export RUSTUP_HOME="${HOME}/.rustup"
export PATH="${CARGO_HOME}/bin:${PATH}"

configure_shell_rc() {
    local shell_rc="${SHELL_RC}"

    local marker_begin="# >>> BOREAL user Rust environment >>>"
    local marker_end="# <<< BOREAL user Rust environment <<<"

    echo
    echo "BOREAL uses a per-user Rust installation:"
    echo
    echo "    CARGO_HOME=${HOME}/.cargo"
    echo "    RUSTUP_HOME=${HOME}/.rustup"
    echo
    echo "BOREAL can add the following configuration to:"
    echo
    echo "    ${shell_rc}"
    echo
    echo "------------------------------------------------------------"
    echo "${marker_begin}"
    echo 'export CARGO_HOME="${HOME}/.cargo"'
    echo 'export RUSTUP_HOME="${HOME}/.rustup"'
    echo 'export PATH="${CARGO_HOME}/bin:${PATH}"'
    echo "${marker_end}"
    echo "------------------------------------------------------------"
    echo

    read -r -p "Update ${shell_rc} to use the per-user Rust environment? [y/N] " answer

    case "${answer}" in
        y|Y|yes|YES|Yes)
            ;;
        *)
            echo "==> Leaving ${shell_rc} unchanged"
            echo
            echo "The current setup script will still use:"
            echo
            echo "    ${CARGO_HOME}"
            echo "    ${RUSTUP_HOME}"
            echo
            echo "Future shells may continue using another Rust installation."
            return
            ;;
    esac

    touch "${shell_rc}"

    #
    # Remove any previous BOREAL-managed block.
    #
    if grep -Fq "${marker_begin}" "${shell_rc}"; then
        echo "==> Replacing existing BOREAL Rust configuration"

        sed -i '' \
            "\|${marker_begin}|,\|${marker_end}|d" \
            "${shell_rc}"
    else
        echo "==> Adding BOREAL Rust configuration"
    fi

    cat >> "${shell_rc}" <<'EOF'

# >>> BOREAL user Rust environment >>>
export CARGO_HOME="${HOME}/.cargo"
export RUSTUP_HOME="${HOME}/.rustup"
export PATH="${CARGO_HOME}/bin:${PATH}"
# <<< BOREAL user Rust environment <<<
EOF

    echo "==> Updated ${shell_rc}"
    echo
    echo "Open a new terminal or run:"
    echo
    echo "    source ${shell_rc}"
}

echo
echo "==> Rust environment for this setup"
echo "    CARGO_HOME:  ${CARGO_HOME}"
echo "    RUSTUP_HOME: ${RUSTUP_HOME}"

#
# Xcode Command Line Tools provide:
#
#     clang
#     ld
#     make
#     SDK headers/libraries
#
# These are required for native macOS builds.
#
echo
echo "==> Checking Xcode Command Line Tools"

if xcode-select -p >/dev/null 2>&1; then
    echo "==> Xcode Command Line Tools already installed"
    echo "    $(xcode-select -p)"
else
    echo "==> Xcode Command Line Tools are not installed"
    echo
    echo "Starting Apple's Command Line Tools installer..."
    echo

    xcode-select --install

    echo
    echo "Complete the Apple Command Line Tools installation,"
    echo "then run this script again."
    exit 0
fi

#
# Verify the compiler is available.
#
echo
echo "==> Checking Apple compiler"

if ! command -v clang >/dev/null 2>&1; then
    echo "ERROR: clang was not found."
    echo
    echo "The Xcode Command Line Tools installation may be incomplete."
    exit 1
fi

echo "    clang: $(command -v clang)"

#
# curl is included with macOS and is required to install
# rustup when no per-user Rust environment exists.
#
echo
echo "==> Checking curl"

if ! command -v curl >/dev/null 2>&1; then
    echo "ERROR: curl is required but was not found."
    exit 1
fi

echo "    curl: $(command -v curl)"

echo
echo "==> Checking user Rust installation"

#
# Check the explicit per-user rustup path rather than
# command -v rustup so a system-wide installation does not
# satisfy this test.
#
if [[ ! -x "${CARGO_HOME}/bin/rustup" ]]; then
    echo "==> Installing Rust using rustup"
    echo "    CARGO_HOME:  ${CARGO_HOME}"
    echo "    RUSTUP_HOME: ${RUSTUP_HOME}"

    curl \
        --proto '=https' \
        --tlsv1.2 \
        -sSf \
        https://sh.rustup.rs \
        | sh -s -- \
            -y \
            --no-modify-path
else
    echo "==> User rustup already installed"
fi

#
# rustup creates ~/.cargo/env.
#
if [[ -f "${CARGO_HOME}/env" ]]; then
    # shellcheck disable=SC1090
    source "${CARGO_HOME}/env"
fi

#
# Reassert the BOREAL per-user Rust environment.
#
export CARGO_HOME="${HOME}/.cargo"
export RUSTUP_HOME="${HOME}/.rustup"
export PATH="${CARGO_HOME}/bin:${PATH}"

#
# Verify the user installation exists.
#
if [[ ! -x "${CARGO_HOME}/bin/rustup" ]]; then
    echo "ERROR: rustup installation failed."
    exit 1
fi

if [[ ! -x "${CARGO_HOME}/bin/cargo" ]]; then
    echo "ERROR: cargo was not installed."
    exit 1
fi

if [[ ! -x "${CARGO_HOME}/bin/rustc" ]]; then
    echo "ERROR: rustc was not installed."
    exit 1
fi

#
# Ensure a default Rust toolchain exists.
#
if ! rustup show active-toolchain >/dev/null 2>&1; then
    echo
    echo "==> Installing stable Rust toolchain"

    rustup toolchain install stable
    rustup default stable
fi

echo
echo "==> Checking Rust targets"

for target in "${TARGETS[@]}"; do
    echo "==> Checking Rust target: ${target}"

    if rustup target list --installed | grep -qx "${target}"; then
        echo "Rust target already installed: ${target}"
    else
        rustup target add "${target}"
    fi
done

#
# Ask whether this user Rust environment should become
# the default for future shell sessions.
#
configure_shell_rc

echo
echo "==> Build environment ready"
echo

echo "Host:"
echo "  Architecture: $(uname -m)"
echo "  macOS:        $(sw_vers -productVersion)"
echo

echo "Apple build tools:"
echo "  clang:        $(command -v clang)"
echo "  SDK path:     $(xcrun --show-sdk-path)"
echo

echo "Rust environment:"
echo "  CARGO_HOME:  ${CARGO_HOME}"
echo "  RUSTUP_HOME: ${RUSTUP_HOME}"
echo

echo "Rust executables used by this script:"
echo "  rustc:  $(command -v rustc)"
echo "  cargo:  $(command -v cargo)"
echo "  rustup: $(command -v rustup)"
echo

rustc --version
cargo --version
rustup --version

echo
echo "Installed Rust targets:"
rustup target list --installed