#!/usr/bin/env bash
# metadata-update.sh - Update NWA metadata in package.json and Markdown files
set -euo pipefail

# -----------------------------
# Resolve paths
# -----------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NWA_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
META_FILE="$NWA_ROOT/metadata.json"

command -v jq >/dev/null || { echo "jq required"; exit 1; }
command -v sed >/dev/null || { echo "sed required"; exit 1; }

# -----------------------------
# Helpers
# -----------------------------
get_meta() {
  jq -r ".METADATA.$1" "$META_FILE"
}

escape_regex() {
  printf '%s' "$1" | sed -e 's/[][\/.^$*+?{}|()]/\\&/g'
}

# -----------------------------
# Update package.json
# -----------------------------
# update_package_json() {
#   local file="$1"
#   local tmp
#   tmp="$(mktemp)"

#   jq \
#     --slurpfile meta "$META_FILE" \
#     '
#     . as $pkg
#     | ($meta.META_MAPS["package.json"] | to_entries) as $maps
#     | reduce $maps[] as $m ($pkg;
#         .[$m.key] = $meta.METADATA[$m.value]
#       )
#     ' "$file" > "$tmp"

#   mv "$tmp" "$file"
#   echo "Updated $file"
# }
update_package_json() {
  local file="$1"
  local tmp
  tmp="$(mktemp)"

  jq \
    --slurpfile meta_file "$META_FILE" \
    '
    ($meta_file[0]) as $meta
    | . as $pkg
    | ($meta.META_MAPS["package.json"] | to_entries) as $maps
    | reduce $maps[] as $m ($pkg;
        .[$m.key] = $meta.METADATA[$m.value]
      )
    ' "$file" > "$tmp"

  mv "$tmp" "$file"
  echo "Updated $file"
}

# -----------------------------
# Update Markdown metadata table
# -----------------------------
update_markdown() {
  local file="$1"

  jq -r '.META_MAPS["*.md"] | to_entries[] | "\(.key)|\(.value)"' "$META_FILE" |
  while IFS='|' read -r label meta_key; do
    value="$(get_meta "$meta_key")"
    esc_label="$(escape_regex "$label")"

    sed -i -E \
    "s#^\\|[[:space:]]*${esc_label}[[:space:]]*\\|[[:space:]]*[^|]*\\|#| ${label} | $value |#" \
    "$file"
  done

  echo "Updated $file"
}

# -----------------------------
# Main
# -----------------------------
jq -r '.MANIFEST[]' "$META_FILE" | while read -r rel_path; do
  file="$NWA_ROOT/$rel_path"

  if [[ ! -f "$file" ]]; then
    echo "WARN: $rel_path not found, skipping"
    continue
  fi

  case "$(basename "$file")" in
    package.json)
      update_package_json "$file"
      ;;
    *.md)
      update_markdown "$file"
      ;;
    *)
      echo "WARN: No handler for $rel_path"
      ;;
  esac
done

echo "Metadata update complete"