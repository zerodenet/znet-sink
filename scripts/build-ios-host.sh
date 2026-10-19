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
export IPHONEOS_DEPLOYMENT_TARGET=16.4
export CARGO_PROFILE_DEV_DEBUG=0
xcodebuild -version > "$evidence/xcode-version.txt"
xcrun --sdk "$sdk" --show-sdk-version > "$evidence/sdk-version.txt"
pnpm tauri ios build --help > "$evidence/locked-cli-build-help.txt"
rustup target add "$rust_target"
pnpm ios:init 2>&1 | tee "$evidence/init.log"
# A packaged Debug build uses embedded assets, not Vite or a CI network URL.
pnpm tauri ios build --debug --target "$cli_target" --ci --no-sign \
  --config '{"build":{"devUrl":null}}' -- --locked 2>&1 | tee "$evidence/app-build.log"
python3 scripts/verify-ios-host.py package --sdk "$sdk" --root src-tauri/gen/apple \
  --output "$evidence/package-receipt.json" --copy "$evidence"
if [[ "$variant" = simulator ]]; then
  python3 scripts/smoke-ios-host.py --evidence "$evidence"
fi
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock src-tauri/crates/mobile-runtime/Cargo.lock
