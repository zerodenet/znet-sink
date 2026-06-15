#!/usr/bin/env bash
# update_version.sh — Bump the project version across all manifest files,
# commit, tag, and push to every configured remote.
#
# Usage:   bash scripts/update_version.sh <version>
# Example: bash scripts/update_version.sh 0.1.0
set -euo pipefail

# ── helpers ──────────────────────────────────────────────────────────────
die() { echo "ERROR: $*" >&2; exit 1; }

# Validate semver shape: digits.digits.digits (optional -suffix)
is_valid_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$ ]]
}

# ── guard ────────────────────────────────────────────────────────────────
VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  die "missing version argument — usage: $0 <version>  (e.g. $0 0.1.0)"
fi

if ! is_valid_version "$VERSION"; then
  die "invalid version '$VERSION' — expected semver, e.g. 0.1.0 or 0.1.0-beta.1"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ── discover manifest files that carry the version ───────────────────────
#
# We update four files:
#   1. package.json              — JS   "version": "..."
#   2. src-tauri/Cargo.toml      — Rust version = "..."
#   3. src-tauri/Cargo.lock      — Rust lock file (gui package entry)
#   4. src-tauri/tauri.conf.json — Tauri "version": "..."
#
# All are updated with a simple sed replacement of the current version
# string.  The script reads the current version from package.json to
# avoid a hard-coded fallback.

CURRENT_VERSION=$(node -p "require('./package.json').version" 2>/dev/null) || \
  CURRENT_VERSION=$(grep '"version"' package.json | head -1 | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')

if [ -z "$CURRENT_VERSION" ]; then
  die "could not determine current version from package.json"
fi

if [ "$CURRENT_VERSION" = "$VERSION" ]; then
  die "version $VERSION is already the current version — nothing to do"
fi

echo "Bumping $CURRENT_VERSION → $VERSION"

# ── update files ─────────────────────────────────────────────────────────
# Use sed -i with a backup extension because macOS sed requires one.
# The backup is deleted immediately after a successful run.
do_sed() {
  local file="$1" pattern="$2"
  if [ ! -f "$file" ]; then
    die "expected manifest file not found: $file"
  fi
  sed -i.bak "$pattern" "$file" && rm -f "${file}.bak"
}

do_sed "package.json" \
  "s/\"version\":[[:space:]]*\"$CURRENT_VERSION\"/\"version\": \"$VERSION\"/"

do_sed "src-tauri/Cargo.toml" \
  "s/^version[[:space:]]*=[[:space:]]*\"$CURRENT_VERSION\"/version = \"$VERSION\"/"

do_sed "src-tauri/tauri.conf.json" \
  "s/\"version\":[[:space:]]*\"$CURRENT_VERSION\"/\"version\": \"$VERSION\"/"

do_sed "src-tauri/Cargo.lock" \
  "s/^version[[:space:]]*=[[:space:]]*\"$CURRENT_VERSION\"/version = \"$VERSION\"/"

echo "Updated manifests:"
echo "  package.json"
echo "  src-tauri/Cargo.toml"
echo "  src-tauri/Cargo.lock"
echo "  src-tauri/tauri.conf.json"

# ── commit & tag ─────────────────────────────────────────────────────────
git add \
  package.json \
  src-tauri/Cargo.toml \
  src-tauri/Cargo.lock \
  src-tauri/tauri.conf.json

COMMIT_MSG="chore(release): bump version to $VERSION"
echo ""
echo "Committing: $COMMIT_MSG"
git commit -m "$COMMIT_MSG"

TAG="v$VERSION"
echo "Tagging: $TAG"
git tag -a "$TAG" -m "$TAG"

# ── push to all remotes ──────────────────────────────────────────────────
REMOTES=$(git remote)
if [ -z "$REMOTES" ]; then
  die "no git remotes configured — cannot push"
fi

echo ""
BRANCH=$(git rev-parse --abbrev-ref HEAD)
for remote in $REMOTES; do
  echo "Pushing $BRANCH → $remote"
  git push "$remote" "$BRANCH"

  echo "Pushing tag $TAG → $remote"
  git push "$remote" "$TAG"
done

echo ""
echo "Done — version $VERSION pushed to all remotes"
