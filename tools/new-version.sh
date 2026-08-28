#!/usr/bin/env bash
# new-version.sh - Create new release branch and update metadata
set -euo pipefail

# -------------------------------------------------
# Preconditions
# -------------------------------------------------
if [[ -n "$(git status --porcelain)" ]]; then
    echo "Working tree is not clean"
    exit 1
fi

# -------------------------------------------------
# Helpers
# -------------------------------------------------
suggest_next_version() {
    local cur
    cur=$(jq -r '.METADATA.version' metadata.json)
    IFS='.' read -ra parts <<< "$cur"
    echo "${parts[0]}.${parts[1]}.$((parts[2] + 1))"
}

# -------------------------------------------------
# Load metadata
# -------------------------------------------------
DESC=$(jq -r '.METADATA.description' metadata.json)
ABBR=$(jq -r '.METADATA.abbr' metadata.json)
COPYRT=$(jq -r '.METADATA.copyright' metadata.json)
CUR_VER=$(jq -r '.METADATA.version' metadata.json)
CUR_VER_DATE=$(jq -r '.METADATA.version_date' metadata.json)
NXT_VER=$(suggest_next_version)

echo "Description:             $DESC ($ABBR)"
echo "Copyright:               (C) $COPYRT"
echo "Current version:         $CUR_VER"
echo "Current version date:    $CUR_VER_DATE"
echo ""

read -p "New version: [$NXT_VER]: " NEW_VER
if [[ -z "$NEW_VER" ]]; then
    NEW_VER="$NXT_VER"
fi

NEW_VER_DATE=$(date +%Y-%m-%d)

echo ""
echo "New version:             $NEW_VER"
echo "New version date:        $NEW_VER_DATE"
echo ""

read -p "Are you sure? (Y/n) " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Nn]$ ]]; then
    exit 1
fi

# -------------------------------------------------
# Safety checks
# -------------------------------------------------
if git show-ref --verify --quiet "refs/heads/v${NEW_VER}"; then
    echo "Branch v${NEW_VER} already exists"
    exit 1
fi

# -------------------------------------------------
# Update metadata.json (structurally)
# -------------------------------------------------
tmp_meta=$(mktemp)
trap 'rm -f "$tmp_meta"' EXIT

jq --arg v "$NEW_VER" --arg d "$NEW_VER_DATE" '
  .METADATA.version = $v
  | .METADATA.version_date = $d
' metadata.json > "$tmp_meta"

mv "$tmp_meta" metadata.json

# -------------------------------------------------
# Create branch
# -------------------------------------------------
git checkout -b "v${NEW_VER}"

# -------------------------------------------------
# Propagate metadata and commit
# -------------------------------------------------
./tools/metadata-update.sh

git commit -am "Start v${NEW_VER}"

echo "Created new release branch v${NEW_VER}"
