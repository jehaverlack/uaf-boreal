#!/usr/bin/env bash
# changelog-update.sh
#
# Idempotently maintain the changelog entry for the active version.
# - Updates METADATA.version_date
# - Syncs maturity
# - Ensures exactly one entry per version
# - Updates or creates the active changelog entry
# - Regenerates docs/CHANGELOG.md

set -euo pipefail

VERSION=$(jq -r '.METADATA.version' metadata.json)
DATE=$(date +%Y-%m-%d)
MATURITY=$(jq -r '.METADATA.maturity' metadata.json)

tmp_meta=$(mktemp)
tmp_changelog=$(mktemp)
trap 'rm -f "$tmp_meta" "$tmp_changelog"' EXIT

# -------------------------------------------------
# Update version_date in metadata.json
# -------------------------------------------------
jq --arg d "$DATE" '
  .METADATA.version_date = $d
' metadata.json > "$tmp_meta"

mv "$tmp_meta" metadata.json

# Propagate metadata into files
./tools/metadata-update.sh

# -------------------------------------------------
# Ensure changelog.json exists and is valid
# -------------------------------------------------
if [[ ! -f changelog.json ]]; then
  echo '{"releases":[]}' > changelog.json
fi

if ! jq -e '.releases | type == "array"' changelog.json >/dev/null 2>&1; then
  echo "ERROR: changelog.json must contain a single 'releases' array"
  exit 1
fi

# -------------------------------------------------
# Normalize: enforce one entry per version (keep newest)
# -------------------------------------------------
# jq '
#   .releases |=
#     ( . | reverse
#       | unique_by(.version)
#       | reverse
#     )
# ' changelog.json > "$tmp_changelog"
jq '
  def semver:
    (.version
     | split(".")
     | map(tonumber));

  .releases |=
    (
      sort_by(semver)        # oldest → newest
      | reverse              # newest → oldest
      | unique_by(.version)  # keep newest entry per version
      | sort_by(semver)
      | reverse
    )
' changelog.json > "$tmp_changelog"


mv "$tmp_changelog" changelog.json

# -------------------------------------------------
# Update or add active version entry
# -------------------------------------------------
if jq -e --arg v "$VERSION" '.releases[] | select(.version == $v)' changelog.json >/dev/null; then
  echo "Updating changelog entry for v${VERSION}"

  jq --arg v "$VERSION" --arg d "$DATE" --arg m "$MATURITY" '
    .releases |=
      map(
        if .version == $v then
          .date = $d
          | .maturity = $m
        else
          .
        end
      )
  ' changelog.json > "$tmp_changelog"
else
  echo "Adding changelog entry for v${VERSION}"

  jq --arg v "$VERSION" --arg d "$DATE" --arg m "$MATURITY" '
    .releases |=
      ([{
        version: $v,
        date: $d,
        maturity: $m,
        summary: "",
        notes: ""
      }] + .)
  ' changelog.json > "$tmp_changelog"
fi

mv "$tmp_changelog" changelog.json

# -------------------------------------------------
# Regenerate Markdown changelog
# -------------------------------------------------
./tools/genmd-changelog.sh
