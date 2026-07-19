#!/usr/bin/env bash
# Bump the release version, commit, tag, and push every configured remote.
#
# Usage:   ./scripts/update_version.sh <version> [--force]
# Example: ./scripts/update_version.sh 0.1.0-rc.1
#          ./scripts/update_version.sh 0.1.0 --force  # rebuild tag at HEAD
set -euo pipefail

die() {
  echo "ERROR: $*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: update_version.sh <version> [--force]

Without --force, synchronizes the release version, commits it, creates an
annotated tag, and pushes the branch and tag to every configured remote.
Prerelease versions such as 0.1.0-rc.1 remain intact for the app and updater;
the script also generates numeric MSI and macOS bundle versions.

With --force, an already-current version skips the bump/commit, rebuilds its
annotated tag at the current HEAD, and force-updates that tag on every remote.
Every release path requires a clean worktree.
EOF
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
  usage
  exit 0
fi

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  die "missing version argument; usage: $0 <version> [--force]"
fi

shift
FORCE=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    --force|-f) FORCE=true ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument '$1'; usage: $0 <version> [--force]" ;;
  esac
  shift
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

node scripts/version-manifests.mjs validate "$VERSION" >/dev/null

CURRENT_VERSION=$(node -p "require('./package.json').version" 2>/dev/null) || \
  die "could not determine current version from package.json"
if [ -z "$CURRENT_VERSION" ]; then
  die "could not determine current version from package.json"
fi

# Refuse to release from a partially synchronized or dirty tree. This prevents
# unrelated local work from being swept into the generated release commit.
node scripts/version-manifests.mjs check "$CURRENT_VERSION" >/dev/null
if [ -n "$(git status --porcelain)" ]; then
  die "release requires a clean worktree"
fi

BRANCH=$(git symbolic-ref --quiet --short HEAD) || \
  die "release requires a named branch (detached HEAD is not supported)"

REMOTES=$(git remote)
if [ -z "$REMOTES" ]; then
  die "no git remotes configured; cannot push"
fi

RETAG_ONLY=false
if [ "$CURRENT_VERSION" = "$VERSION" ]; then
  if [ "$FORCE" != true ]; then
    die "version $VERSION is already current; use --force to rebuild and republish its tag"
  fi
  RETAG_ONLY=true
  echo "Version $VERSION is already current; rebuilding its tag at HEAD"
else
  if [ "$FORCE" = true ]; then
    die "--force only rebuilds the already-current version tag; omit it for a normal version bump"
  fi
  node scripts/version-manifests.mjs assert-newer "$VERSION" "$CURRENT_VERSION" >/dev/null
  echo "Bumping $CURRENT_VERSION -> $VERSION"
fi

TAG="v$VERSION"

# A normal bump must never discover a conflicting tag after it has already
# created the release commit. Probe local and remote tags before changing files.
if [ "$FORCE" != true ]; then
  if git rev-parse --quiet --verify "refs/tags/$TAG" >/dev/null; then
    die "tag $TAG already exists locally"
  fi
  for remote in $REMOTES; do
    if git ls-remote --exit-code --tags "$remote" "refs/tags/$TAG" >/dev/null 2>&1; then
      die "tag $TAG already exists on remote $remote"
    else
      probe_status=$?
      if [ "$probe_status" -ne 2 ]; then
        die "failed to check tag $TAG on remote $remote"
      fi
    fi
  done
fi

MANIFESTS=(
  package.json
  src-tauri/Cargo.toml
  src-tauri/Cargo.lock
  src-tauri/tauri.conf.json
  src-tauri/Info.plist
)

ROLLBACK_MANIFESTS=false
rollback_on_error() {
  status=$?
  if [ "$status" -ne 0 ] && [ "$ROLLBACK_MANIFESTS" = true ]; then
    echo "Release preparation failed; restoring version manifests" >&2
    git restore --staged --worktree -- "${MANIFESTS[@]}" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap rollback_on_error EXIT

if [ "$RETAG_ONLY" != true ]; then
  ROLLBACK_MANIFESTS=true
  node scripts/version-manifests.mjs set "$VERSION"

  # Cargo owns Cargo.lock. Never replace version strings in the lockfile: a
  # blanket replacement can corrupt third-party dependency versions.
  echo "Syncing src-tauri/Cargo.lock via cargo..."
  cargo check --manifest-path src-tauri/Cargo.toml -q
  node scripts/version-manifests.mjs check "$VERSION"
  git diff --check

  git add "${MANIFESTS[@]}"
  if git diff --cached --quiet; then
    die "version update produced no manifest changes"
  fi

  COMMIT_MSG="chore(release): bump version to $VERSION"
  echo ""
  echo "Committing: $COMMIT_MSG"
  git commit -m "$COMMIT_MSG"
  ROLLBACK_MANIFESTS=false
fi

if [ "$FORCE" = true ]; then
  echo "Force-tagging current HEAD: $TAG"
  git tag -fa "$TAG" -m "$TAG"
else
  echo "Tagging: $TAG"
  git tag -a "$TAG" -m "$TAG"
fi

echo ""
for remote in $REMOTES; do
  echo "Pushing $BRANCH -> $remote"
  git push "$remote" "$BRANCH"

  if [ "$FORCE" = true ]; then
    echo "Force-pushing tag $TAG -> $remote"
    git push --force "$remote" "refs/tags/$TAG:refs/tags/$TAG"
  else
    echo "Pushing tag $TAG -> $remote"
    git push "$remote" "refs/tags/$TAG:refs/tags/$TAG"
  fi
done

echo ""
echo "Done; release $VERSION pushed to all remotes"
