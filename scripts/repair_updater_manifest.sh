#!/usr/bin/env bash
# repair_updater_manifest.sh — Re-publish a valid latest.json for an existing
# release so installed clients stop failing update checks with
# "missing field `version`".
#
# This is a one-time repair for releases that were built without
# TAURI_SIGNING_PRIVATE_KEY: the published latest.json is an invalid
# {"platforms":{}} placeholder, which breaks the Tauri updater plugin's
# deserialization on every client.
#
# The repair uploads a minimal but *valid* manifest whose version matches
# the release tag.  Installed clients at that version will then see
# "up to date" instead of erroring.  Real signed updates become available
# once a subsequent release is built WITH the signing key configured.
#
# Prerequisites:
#   - GitHub CLI (gh) authenticated with repo access
#
# Usage:
#   bash scripts/repair_updater_manifest.sh <tag>
#   bash scripts/repair_updater_manifest.sh v0.0.5
set -euo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }

TAG="${1:-}"
[ -n "$TAG" ] || die "Usage: bash scripts/repair_updater_manifest.sh <tag>  (e.g. v0.0.5)"

# Strip leading 'v' to get the bare version number.
VERSION="${TAG#v}"

command -v gh >/dev/null 2>&1 || die "GitHub CLI (gh) is required. Install from https://cli.github.com"

echo "Generating valid latest.json for $TAG (version $VERSION)…"

# Minimal valid Tauri updater manifest.  platforms is empty because this
# release has no signed bundles — but the top-level version/pub_date are
# present so the updater plugin can deserialize it.
cat > latest.json <<EOF
{
  "version": "${VERSION}",
  "notes": "",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {}
}
EOF

echo "--- latest.json ---"
cat latest.json
echo "-------------------"

echo ""
echo "Uploading to release $TAG (replaces existing latest.json)…"
gh release upload "$TAG" latest.json --clobber

echo ""
echo "✓ Done.  Installed clients on ${VERSION} will now report 'up to date'"
echo "  instead of failing update checks."
echo ""
echo "Note: real signed updates require rebuilding with TAURI_SIGNING_PRIVATE_KEY"
echo "configured — see .github/workflows/release.yml."

rm -f latest.json
