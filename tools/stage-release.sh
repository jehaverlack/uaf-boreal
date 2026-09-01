#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

APP="boreal"
BUILD_DIR="build"
DIST_DIR="dist"

command -v jq >/dev/null 2>&1 || {
    echo "ERROR: jq is required."
    exit 1
}

if [[ -n "$(git status --porcelain)" ]]; then
    echo "ERROR: Commit or remove working-tree changes before staging a release."
    exit 1
fi

VERSION="$(jq -r '.METADATA.version' metadata.json)"

if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
    echo "ERROR: Unable to read METADATA.version from metadata.json"
    exit 1
fi

CARGO_VERSION="$(sed -n -E 's/^version[[:space:]]*=[[:space:]]*"([^"]+)"/\1/p' Cargo.toml | head -n 1)"

if [[ "$CARGO_VERSION" != "$VERSION" ]]; then
    echo "ERROR: Cargo.toml version '${CARGO_VERSION}' does not match metadata version '${VERSION}'."
    exit 1
fi

if ! jq -e --arg version "$VERSION" \
    '.releases[] | select(.version == $version)' \
    changelog.json >/dev/null
then
    echo "ERROR: changelog.json has no release entry for v${VERSION}."
    echo "Run ./tools/changelog-update.sh and review the release entry first."
    exit 1
fi

declare -A TARGET_FILES=(
    ["x86_64-unknown-linux-gnu"]="${APP}-v${VERSION}-linux-x86_64"
    ["aarch64-unknown-linux-gnu"]="${APP}-v${VERSION}-linux-aarch64"
    ["armv7-unknown-linux-gnueabihf"]="${APP}-v${VERSION}-linux-armv7"
    ["x86_64-pc-windows-gnu"]="${APP}-v${VERSION}-windows-x86_64.exe"
    ["x86_64-apple-darwin"]="${APP}-v${VERSION}-macos-x86_64"
    ["aarch64-apple-darwin"]="${APP}-v${VERSION}-macos-aarch64"
)

TARGET_ORDER=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "armv7-unknown-linux-gnueabihf"
    "x86_64-pc-windows-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
)

EXPECTED_FILES=()
MISSING_FILES=()

remove_old_release_artifacts() {
    local dist_path
    local artifact
    local filename
    local artifact_version
    local -a old_artifacts=()

    dist_path="$(cd "$DIST_DIR" && pwd -P)"

    if [[ "$dist_path" != "$PROJECT_ROOT/dist" ]]; then
        echo "ERROR: Refusing to clean unexpected dist directory: $dist_path"
        return 1
    fi

    while IFS= read -r -d '' artifact; do
        filename="$(basename "$artifact")"

        if [[ "$filename" =~ ^boreal-v([0-9]+\.[0-9]+\.[0-9]+)-(linux-(x86_64|aarch64|armv7)|windows-x86_64\.exe|macos-(x86_64|aarch64))$ ]]; then
            artifact_version="${BASH_REMATCH[1]}"

            if [[ "$artifact_version" != "$VERSION" ]]; then
                old_artifacts+=("$artifact")
            fi
        fi
    done < <(find "$dist_path" -maxdepth 1 -type f -name 'boreal-v*' -print0)

    if (( ${#old_artifacts[@]} == 0 )); then
        return
    fi

    echo "==> Removing old release artifacts from dist"

    for artifact in "${old_artifacts[@]}"; do
        echo "  Removing ${artifact#"$PROJECT_ROOT/"}"
        rm -- "$artifact"
    done
}

for target in "${TARGET_ORDER[@]}"; do
    if jq -e --arg target "$target" '.BUILD_TARGETS[$target] == true' metadata.json \
        >/dev/null
    then
        file="${TARGET_FILES[$target]}"
        EXPECTED_FILES+=("$file")

        if [[ ! -f "${BUILD_DIR}/${file}" ]]; then
            MISSING_FILES+=("${BUILD_DIR}/${file}")
        fi
    fi
done

if (( ${#MISSING_FILES[@]} > 0 )); then
    echo "ERROR: Release artifact set for v${VERSION} is incomplete:"

    for file in "${MISSING_FILES[@]}"; do
        echo "  Missing: ${file}"
    done

    echo
    echo "Build artifacts on their required hosts, copy them into build/,"
    echo "and run this script again."
    exit 1
fi

mkdir -p "$DIST_DIR"

echo "==> Staging BOREAL v${VERSION} release artifacts"

for file in "${EXPECTED_FILES[@]}"; do
    cp -p "${BUILD_DIR}/${file}" "${DIST_DIR}/${file}"
    echo "  ${DIST_DIR}/${file}"
done

CHECKSUM_FILE="${DIST_DIR}/SHA256SUMS"

if command -v sha256sum >/dev/null 2>&1; then
    (
        cd "$DIST_DIR"
        sha256sum "${EXPECTED_FILES[@]}" > "$(basename "$CHECKSUM_FILE")"
    )
elif command -v shasum >/dev/null 2>&1; then
    (
        cd "$DIST_DIR"
        shasum -a 256 "${EXPECTED_FILES[@]}" > "$(basename "$CHECKSUM_FILE")"
    )
else
    echo "ERROR: sha256sum or shasum is required to generate checksums."
    exit 1
fi

remove_old_release_artifacts

echo "  ${CHECKSUM_FILE}"
echo
echo "==> Release staging complete"
ls -lh "$DIST_DIR"
