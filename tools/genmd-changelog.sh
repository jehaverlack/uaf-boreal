#!/usr/bin/env bash
# genmd-changelog.sh
#
# Generate docs/CHANGELOG.md from changelog.json, metadata.json,
# and Git merge history.

set -euo pipefail

OUT_FILE="docs/CHANGELOG.md"

echo "Generating ${OUT_FILE}"

# -------------------------------------------------
# Load metadata for header
# -------------------------------------------------
NAME=$(jq -r '.METADATA.description' metadata.json)
ABBR=$(jq -r '.METADATA.abbr' metadata.json)
VERSION=$(jq -r '.METADATA.version' metadata.json)
DATE=$(jq -r '.METADATA.version_date' metadata.json)
AUTHOR=$(jq -r '.METADATA.author' metadata.json)
COPYRIGHT=$(jq -r '.METADATA.copyright' metadata.json)
LICENSE=$(jq -r '.METADATA.license' metadata.json)

# -------------------------------------------------
# Collect release merge commits (newest → oldest)
# -------------------------------------------------
mapfile -t MERGES < <(
  git log main --merges --pretty=format:'%H|%s' |
  grep -E "(Merge branch 'v|Release v)"
)

find_merge() {
  local tag="v$1"
  for entry in "${MERGES[@]}"; do
    if [[ "$entry" == *"Merge branch '${tag}'"* ]] || [[ "$entry" == *"Release ${tag}"* ]]; then
      echo "${entry%%|*}"
      return
    fi
  done
}

# -------------------------------------------------
# Write header (README-style)
# -------------------------------------------------
{
  echo "# ${NAME} (${ABBR})"
  echo ""
  echo "| Attribute | Value |"
  echo "| --- | --- |"
  echo "| **Author** | ${AUTHOR} |"
  echo "| **Copyright** | ${COPYRIGHT} |"
  echo "| **License** | ${LICENSE} |"
  echo "| **Version** | ${VERSION} |"
  echo "| **Date** | ${DATE} |"
  echo ""
} > "$OUT_FILE"

# -------------------------------------------------
# Iterate releases (SEMVER newest → oldest)
# -------------------------------------------------
jq -r '
  def semver:
    (.version | split(".") | map(tonumber));

  .releases
  | sort_by(semver)
  | reverse
  | .[]
  | "\(.version)|\(.date)|\(.maturity)|\(.summary)|\(.notes)"
' changelog.json |
while IFS='|' read -r version rdate maturity summary notes; do
  tag="v${version}"
  merge_commit=$(find_merge "$version")

  echo "## ${tag} - ${rdate} (${maturity})" >> "$OUT_FILE"
  echo "" >> "$OUT_FILE"

  if [[ -n "$summary" ]]; then
    echo "**Summary**" >> "$OUT_FILE"
    echo "$summary" >> "$OUT_FILE"
    echo "" >> "$OUT_FILE"
  fi

  if [[ -n "$notes" ]]; then
    echo "**Notes**" >> "$OUT_FILE"
    echo "$notes" >> "$OUT_FILE"
    echo "" >> "$OUT_FILE"
  fi

  # -------------------------------------------------
  # Commit list
  # -------------------------------------------------
  if [[ -n "$merge_commit" ]]; then
    # Released version: merge-boundary commits
    lower_bound=""
    for ((i=0; i<${#MERGES[@]}; i++)); do
      if [[ "${MERGES[$i]}" == "${merge_commit}"* ]]; then
        if (( i + 1 < ${#MERGES[@]} )); then
          lower_bound="${MERGES[$((i+1))]%%|*}"
        fi
        break
      fi
    done

    if [[ -n "$lower_bound" ]]; then
      git log "${lower_bound}..${merge_commit}" \
        --no-merges \
        --pretty=format:"- %s" >> "$OUT_FILE"
    else
      git log "${merge_commit}" \
        --no-merges \
        --pretty=format:"- %s" >> "$OUT_FILE"
    fi
  else
    # Active (not yet merged) version: commits since last release
    if [[ ${#MERGES[@]} -gt 0 ]]; then
      last_merge="${MERGES[0]%%|*}"
      git log "${last_merge}..HEAD" \
        --no-merges \
        --pretty=format:"- %s" >> "$OUT_FILE"
    else
      git log HEAD \
        --no-merges \
        --pretty=format:"- %s" >> "$OUT_FILE"
    fi
  fi

  echo "" >> "$OUT_FILE"
  echo "" >> "$OUT_FILE"
done

cp docs/CHANGELOG.md nwa/html/md/changelog.md

echo "Changelog written to ${OUT_FILE}"