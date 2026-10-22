#!/usr/bin/env bash
# Complete standard Tauri application, never a NativeIo-only library result.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" = Darwin ]] || { echo 'iOS application builds require the macOS Xcode SDK.' >&2; exit 1; }
variant="${1:-simulator}"
case "$variant" in
  simulator) sdk=iphonesimulator; case "$(uname -m)" in arm64) rust_target=aarch64-apple-ios-sim; cli_target=aarch64-sim ;; *) rust_target=x86_64-apple-ios; cli_target=x86_64 ;; esac ;;
  device) sdk=iphoneos; rust_target=aarch64-apple-ios; cli_target=aarch64 ;;
  *) echo 'Expected simulator or device' >&2; exit 1 ;;
esac
# CI does not import, create or select a signing identity or contact provisioning.
for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH IOS_CERTIFICATE IOS_CERTIFICATE_PASSWORD IOS_MOBILE_PROVISION APPLE_DEVELOPMENT_TEAM; do
  [[ -z "${!name:-}" ]] || { echo "Refusing signing environment variable: $name" >&2; exit 1; }
done
evidence="${ZNET_IOS_EVIDENCE_DIR:-$PWD/ios-host-evidence/$variant}"
[[ ! -e "$evidence" ]] || { echo 'Use a fresh evidence directory to avoid stale success.' >&2; exit 1; }
mkdir -p "$evidence"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
# A fresh target directory binds each SwiftPM workspace to this SDK build;
# never choose among stale build-script outputs using their modification time.
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/znet-ios-$variant.XXXXXX")"
export IPHONEOS_DEPLOYMENT_TARGET=16.4
# swift-rs chooses Debug from Cargo's DEBUG (debug-info enabled), not Xcode's
# configuration. Keep limited symbols so the nonce-gated Debug smoke is built.
export CARGO_PROFILE_DEV_DEBUG=1
xcodebuild -version > "$evidence/xcode-version.txt"
xcrun --sdk "$sdk" --show-sdk-version > "$evidence/sdk-version.txt"
pnpm tauri ios build --help > "$evidence/locked-cli-build-help.txt"
rustup target add "$rust_target"
cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 --locked \
  --filter-platform "$rust_target" > "$evidence/cargo-metadata.json"
node scripts/license-rust.mjs --target "$rust_target" --profile debug \
  --features tool-dns,tool-route,tool-node-probe --output "$evidence/rust-license.json"
export ZNET_LICENSE_INVENTORY="$evidence/rust-license.json"
export ZNET_REQUIRE_LICENSE_INVENTORY=1
# The locked mobile CLI explicitly forwards TAURI_* into Xcode's Rust build.
export TAURI_ZNET_LICENSE_INVENTORY="$ZNET_LICENSE_INVENTORY"
export TAURI_ZNET_REQUIRE_LICENSE_INVENTORY="$ZNET_REQUIRE_LICENSE_INVENTORY"
pnpm ios:init 2>&1 | tee "$evidence/init.log"
# A packaged Debug build uses embedded assets, not Vite or a CI network URL.
pnpm tauri ios build --debug --target "$cli_target" --ci --no-sign \
  --config '{"build":{"devUrl":null}}' -- --locked 2>&1 | tee "$evidence/input-build.log"
node scripts/license-ios-app.mjs --cargo "$evidence/rust-license.json" \
  --frontend build/license-inventory.frontend.json --metadata "$evidence/cargo-metadata.json" \
  --target-dir "$CARGO_TARGET_DIR" --sdk-path "$(xcrun --sdk "$sdk" --show-sdk-path)" \
  --evidence "$evidence/swift-inventories" --output "$evidence/license-inventory.native.json"
node --input-type=module - "$evidence" <<'JS'
import fs from 'node:fs';
import path from 'node:path';
const evidence = process.argv[2];
fs.writeFileSync(path.join(evidence, 'license-config.json'), JSON.stringify({
  // Freeze the frontend already used by the input SDK build and inventory.
  // Another Vite build can change rendered-chunk evidence even at one source
  // revision. Keep strict freshness checks instead of comparing stale receipts.
  build: {devUrl: null, beforeBuildCommand: ''}, bundle: {resources: {
    'resources/builtin-rules/NOTICE.md': 'resources/builtin-rules/NOTICE.md',
    'resources/builtin-rules/LICENSE.meta-rules-dat': 'resources/builtin-rules/LICENSE.meta-rules-dat',
    [path.join(evidence, 'license-inventory.native.json')]: 'license-inventory.native.json'
  }}
}));
JS
# Tauri's archive export renames the product App. A second export into the
# same archive can collide with that first renamed bundle. Preserve this
# run's input products as evidence and give the final CLI export a fresh root.
mv src-tauri/gen/apple/build "$evidence/input-sdk-products"
# Rebuild through the same CLI resource pipeline. Do not inject legal assets
# into the produced App afterwards, or reuse the first build as final evidence.
pnpm tauri ios build --debug --target "$cli_target" --ci --no-sign \
  --config "$evidence/license-config.json" -- --locked 2>&1 | tee "$evidence/app-build.log"
python3 scripts/verify-ios-host.py package --sdk "$sdk" --root src-tauri/gen/apple \
  --output "$evidence/package-receipt.json" --copy "$evidence"
node scripts/license-ios-artifact.mjs --app "$evidence/ZNetSink.app" \
  --expected "$evidence/license-inventory.native.json" --frontend build/license-inventory.frontend.json \
  --output "$evidence/license-package-receipt.json"
if [[ "$variant" = simulator ]]; then
  python3 scripts/smoke-ios-host.py --evidence "$evidence"
fi
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock src-tauri/crates/mobile-runtime/Cargo.lock
