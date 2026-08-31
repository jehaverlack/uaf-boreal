#!/usr/bin/env bash
# main-merge.sh - Merge release branch into main, tag, and delete branch
set -euo pipefail

CUR_VER=$(jq -r '.METADATA.version' metadata.json)
CUR_BRANCH=$(git branch --show-current)
TAG="v${CUR_VER}"

# Ensure correct branch
if [[ "$CUR_BRANCH" != "$TAG" ]]; then
    echo "You must be on branch $TAG"
    exit 1
fi

# Ensure clean working tree
if [[ -n "$(git status --porcelain)" ]]; then
    echo "There are uncommitted changes"
    exit 1
fi

# Ensure tag does not already exist
if git show-ref --verify --quiet "refs/tags/$TAG"; then
    echo "Tag $TAG already exists"
    exit 1
fi


# Switch to main
git checkout main
if [[ "$(git branch --show-current)" != "main" ]]; then
    echo "Failed to switch to main"
    exit 1
fi

# Merge release branch
git merge --no-ff "$CUR_BRANCH" -m "Release $TAG"

# Create annotated tag
git tag -a "$TAG" -m "Release $TAG"

# Delete release branch
git branch -d "$CUR_BRANCH"

echo "Release $TAG merged, tagged, and branch deleted"
