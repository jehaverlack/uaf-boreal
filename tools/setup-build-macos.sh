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

#
# This script configures a non-root user's BOREAL build
# environment.
#
# System packages are installed using sudo.
#
# Rust itself is installed and managed per-user under:
#
#     ~/.cargo
#     ~/.rustup
#
# Any existing system-wide Rust installation such as
# /opt/cargo or /opt/rustup is left unchanged.
#

if [[ "${EUID}" -eq 0 ]]; then
    echo "ERROR: Do not run this script as root."
    echo
    echo "Run it as the user who will build BOREAL:"
    echo
    echo "    ./tools/setup-build-linux.sh"
    echo
    exit 1
fi

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "ERROR: This script is intended for Linux."
    exit 1
fi

if ! command -v apt >/dev/null 2>&1; then
    echo "ERROR: This setup script currently supports Debian/Ubuntu systems using apt."
    exit 1
fi

#
# BOREAL uses a per-user Rust environment.
#
# These intentionally override any system-wide values that
# may already exist in the current shell.
#
export CARGO_HOME="${HOME}/.cargo"
export RUSTUP_HOME="${HOME}/.rustup"
export PATH="${CARGO_HOME}/bin:${PATH}"

configure_bashrc() {
    local bashrc="${HOME}/.bashrc"

    local marker_begin="# >>> BOREAL user Rust environment >>>"
    local marker_end="# <<< BOREAL user Rust environment <<<"

    echo
    echo "BOREAL uses a per-user Rust installation:"
    echo
    echo "    CARGO_HOME=${HOME}/.cargo"
    echo "    RUSTUP_HOME=${HOME}/.rustup"
    echo
    echo "Your current shell may be configured to use a"
    echo "system-wide Rust installation instead."
    echo
    echo "BOREAL can add the following configuration to:"
    echo
    echo "    ${bashrc}"
    echo
    echo "------------------------------------------------------------"
    echo "${marker_begin}"
    echo 'export CARGO_HOME="${HOME}/.cargo"'
    echo 'export RUSTUP_HOME="${HOME}/.rustup"'
    echo 'export PATH="${CARGO_HOME}/bin:${PATH}"'
    echo "${marker_end}"
    echo "------------------------------------------------------------"
    echo

    read -r -p "Update ${bashrc} to use the per-user Rust environment? [y/N] " answer

    case "${answer}" in
        y|Y|yes|YES|Yes)
            ;;
        *)
            echo "==> Leaving ${bashrc} unchanged"
            echo
            echo "The current setup script will still use:"
            echo
            echo "    ${CARGO_HOME}"
            echo "    ${RUSTUP_HOME}"
            echo
            echo "Future shells may continue using the system-wide Rust installation."
            return
            ;;
    esac

    touch "${bashrc}"

    #
    # Remove any previous BOREAL-managed block.
    #
    # This keeps the operation idempotent if the setup script
    # is run more than once.
    #
    if grep -Fq "${marker_begin}" "${bashrc}"; then
        echo "==> Replacing existing BOREAL Rust configuration"

        sed -i \
            "\|${marker_begin}|,\|${marker_end}|d" \
            "${bashrc}"
    else
        echo "==> Adding BOREAL Rust configuration"
    fi

    cat >> "${bashrc}" <<'EOF'

# >>> BOREAL user Rust environment >>>
export CARGO_HOME="${HOME}/.cargo"
export RUSTUP_HOME="${HOME}/.rustup"
export PATH="${CARGO_HOME}/bin:${PATH}"
# <<< BOREAL user Rust environment <<<
EOF

    echo "==> Updated ${bashrc}"
    echo
    echo "Open a new terminal or run:"
    echo
    echo "    source ~/.bashrc"
}

echo
echo "==> Rust environment for this setup"
echo "    CARGO_HOME:  ${CARGO_HOME}"
echo "    RUSTUP_HOME: ${RUSTUP_HOME}"

echo
echo "==> Installing system build dependencies"

sudo apt update

sudo apt install -y \
    build-essential \
    pkg-config \
    curl \
    jq \
    gcc-mingw-w64-x86-64 \
    gcc-aarch64-linux-gnu \
    gcc-arm-linux-gnueabihf

echo
echo "==> Checking user Rust installation"

#
# Check the explicit per-user rustup path rather than using:
#
#     command -v rustup
#
# because that could discover a system-wide installation
# such as /opt/cargo/bin/rustup.
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
# Source it for this script when available.
#
if [[ -f "${CARGO_HOME}/env" ]]; then
    # shellcheck disable=SC1090
    source "${CARGO_HOME}/env"
fi

#
# Reassert the BOREAL per-user Rust environment after
# sourcing rustup's environment file.
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
# This handles the case where ~/.cargo and ~/.rustup
# already exist but do not contain a usable compiler
# toolchain.
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
# Ask whether this per-user Rust environment should also
# become the user's default in future Bash sessions.
#
configure_bashrc

echo
echo "==> Build environment ready"
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