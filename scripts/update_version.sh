#!/usr/bin/env bash
# Prepare and publish a release according to the branch-aware release policy.
#
# develop: ./scripts/update_version.sh 0.1.0    -> 0.1.0-dev.YYYYMMDDHHmm
# main:    ./scripts/update_version.sh 0.1.0-rc -> 0.1.0-rc.YYYYMMDDHHmm
# main:    ./scripts/update_version.sh 0.1.0    -> 0.1.0
set -euo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }
usage() {
  cat <<'USAGE'
Usage: update_version.sh <X.Y.Z|X.Y.Z-rc>

Version input never includes the Git tag 'v' prefix.
- develop accepts X.Y.Z and automatically publishes X.Y.Z-dev.<UTC timestamp>.
- main accepts X.Y.Z-rc for a release candidate or X.Y.Z for stable.

Release history is forward-only: dev -> rc -> stable. Stable release lines are
sealed permanently and stable tags are never rewritten or deleted.
USAGE
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then usage; exit 0; fi
REQUESTED_VERSION="${1:-}"
[ -n "$REQUESTED_VERSION" ] || die "missing version argument; usage: $0 <X.Y.Z|X.Y.Z-rc>"
[ "$#" -eq 1 ] || die "unexpected arguments; usage: $0 <X.Y.Z|X.Y.Z-rc>"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

CURRENT_VERSION=$(node -p "require('./package.json').version" 2>/dev/null) || die "could not determine current version"
node scripts/version-manifests.mjs check "$CURRENT_VERSION" >/dev/null
[ -z "$(git status --porcelain)" ] || die "release requires a clean worktree"

PLAN_JSON=$(node scripts/release-policy.mjs plan "$REQUESTED_VERSION") || exit $?
read_json() { node -e "const p=JSON.parse(process.argv[1]); process.stdout.write(String(p[process.argv[2]] ?? ''))" "$PLAN_JSON" "$1"; }
VERSION=$(read_json releaseVersion)
TAG=$(read_json tag)
CHANNEL=$(read_json channel)
BUILD_NUMBER=$(read_json buildNumber)
BRANCH=$(read_json branch)

if [ "$CHANNEL" = stable ]; then node scripts/check-stable-readiness.mjs; fi

[ "$CURRENT_VERSION" != "$VERSION" ] || die "release version $VERSION is already current; release tags are immutable"
node scripts/version-manifests.mjs assert-newer "$VERSION" "$CURRENT_VERSION" >/dev/null

REMOTES=$(git remote)
printf '%s\n' "$REMOTES" | grep -qx origin || die "release authority remote 'origin' is required"
if git rev-parse --quiet --verify "refs/tags/$TAG" >/dev/null; then die "tag $TAG already exists locally"; fi
for remote in $REMOTES; do
  if git ls-remote --exit-code --tags "$remote" "refs/tags/$TAG" >/dev/null 2>&1; then
    die "tag $TAG already exists on remote $remote"
  else
    probe_status=$?
    [ "$probe_status" -eq 2 ] || die "failed to check tag $TAG on remote $remote"
  fi
done

MANIFESTS=(package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/Info.plist)
BASE_HEAD=$(git rev-parse HEAD)
ROLLBACK_MANIFESTS=true
rollback_on_error() {
  status=$?
  if [ "$status" -ne 0 ] && [ "$ROLLBACK_MANIFESTS" = true ]; then
    git restore --staged --worktree -- "${MANIFESTS[@]}" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap rollback_on_error EXIT

printf 'Release plan: %s -> %s (%s, native build %s)\n' "$REQUESTED_VERSION" "$VERSION" "$CHANNEL" "$BUILD_NUMBER"
node scripts/version-manifests.mjs set "$VERSION" "$BUILD_NUMBER"
echo "Syncing src-tauri/Cargo.lock via cargo..."
cargo check --manifest-path src-tauri/Cargo.toml -q
node scripts/version-manifests.mjs check "$VERSION" "$BUILD_NUMBER"
git diff --check

git add "${MANIFESTS[@]}"
git diff --cached --quiet && die "version update produced no manifest changes"
COMMIT_MSG="chore(release): $VERSION"
git commit -m "$COMMIT_MSG"
ROLLBACK_MANIFESTS=false

git tag -a "$TAG" -m "$TAG"

echo "Publishing $BRANCH and $TAG atomically to release authority origin..."
if ! git push --atomic origin "$BRANCH" "refs/tags/$TAG:refs/tags/$TAG"; then
  echo "Authority push failed; restoring the local pre-release state" >&2
  git tag -d "$TAG" >/dev/null 2>&1 || true
  git reset --hard "$BASE_HEAD" >/dev/null
  die "failed to publish release to origin"
fi

MIRROR_FAILURES=()
for remote in $REMOTES; do
  [ "$remote" = origin ] && continue
  echo "Mirroring $BRANCH and $TAG -> $remote"
  if ! git push --atomic "$remote" "$BRANCH" "refs/tags/$TAG:refs/tags/$TAG"; then
    MIRROR_FAILURES+=("$remote")
    echo "WARNING: mirror $remote failed; origin remains the release authority" >&2
  fi
done

echo "Done; release $VERSION submitted to origin"
if [ "${#MIRROR_FAILURES[@]}" -gt 0 ]; then
  echo "WARNING: mirror sync failed for: ${MIRROR_FAILURES[*]}" >&2
fi
