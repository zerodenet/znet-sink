#!/usr/bin/env bash
# Installable development host only. Never retain a development signing key.
set -euo pipefail
umask 077
cd "$(dirname "$0")/.."
root="$PWD"
target="${ZNET_ANDROID_DEBUG_TARGET:-aarch64}"
target_spec=$(node scripts/android-build-target.mjs "$target")
read -r rust_target expected_abi gradle_arch <<< "$target_spec"
if test -e android-debug-host-evidence; then
  echo 'Move the previous android-debug-host-evidence directory before a new run; stale success evidence must not be reused.' >&2
  exit 1
fi
state=$(mktemp -d "${ZNET_DEBUG_STATE_PARENT:-${RUNNER_TEMP:-/tmp}}/znet-debug.XXXXXXXX")
trap 'rm -rf -- "$state"' EXIT
export ANDROID_USER_HOME="$state/android-user"
# AGP rejects multiple preference-location variables pointing at different paths.
# The explicit Debug storeFile and this one current SDK location own all state.
unset ANDROID_SDK_HOME ANDROID_PREFS_ROOT
export GRADLE_USER_HOME="$state/gradle"
mkdir -p "$ANDROID_USER_HOME" "$GRADLE_USER_HOME" android-debug-host-evidence
printf '%s\n' 'org.gradle.daemon=false' > "$GRADLE_USER_HOME/gradle.properties"
# A fresh private Gradle home must not silently reuse any user's signing state.
export CARGO_INCREMENTAL=0
export PYTHONDONTWRITEBYTECODE=1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_DEV_DEBUG=0
export ZNET_PRODUCT_FEATURES="${ZNET_PRODUCT_FEATURES:-tool-dns,tool-route,tool-node-probe}"
: "${ANDROID_HOME:?Android SDK required}"
: "${ANDROID_NDK_HOME:?Pinned Android NDK required}"
source scripts/android-plugin-toolchain.sh
: "${JAVA_HOME:?JDK required}"
test -x "$JAVA_HOME/bin/java"
test -x "$JAVA_HOME/bin/javac"
test -x "$JAVA_HOME/bin/keytool"
"$JAVA_HOME/bin/java" -version
grep -Eq '^Pkg.Revision[[:space:]]*=[[:space:]]*27\.3\.13750724[[:space:]]*$' "$ANDROID_NDK_HOME/source.properties"
tools="$ANDROID_HOME/build-tools/35.0.0"
test -x "$tools/apksigner"
test -x "$tools/aapt"
test -d "$ANDROID_HOME/platforms/android-36"
python3 scripts/test-debug-android-apk.py 2>&1 | tee android-debug-host-evidence/verifier-tests.log
pnpm test:licenses 2>&1 | tee android-debug-host-evidence/license-tests.log
pnpm build 2>&1 | tee android-debug-host-evidence/frontend-build.log
node scripts/license-rust.mjs --target "$rust_target" --profile debug --features "$ZNET_PRODUCT_FEATURES" --output "$state/rust-license.json"
export ZNET_LICENSE_INVENTORY="$state/rust-license.json"
export ZNET_REQUIRE_LICENSE_INVENTORY=1
export TAURI_ZNET_LICENSE_INVENTORY="$ZNET_LICENSE_INVENTORY"
export TAURI_ZNET_REQUIRE_LICENSE_INVENTORY="$ZNET_REQUIRE_LICENSE_INVENTORY"
export ZNET_LICENSE_FRONTEND="$root/build/license-inventory.frontend.json"
export ORG_GRADLE_PROJECT_znetLicenseMergeScript="$root/scripts/license-native-merge.mjs"
export ORG_GRADLE_PROJECT_znetLicenseCargoInventory="$ZNET_LICENSE_INVENTORY"
export ORG_GRADLE_PROJECT_znetLicenseFrontendInventory="$ZNET_LICENSE_FRONTEND"
export ORG_GRADLE_PROJECT_znetLicenseMaterialsCache="$root/resources/licenses/authoritative-materials.json"
export ORG_GRADLE_PROJECT_znetLicenseConfiguration=universalDebugRuntimeClasspath
export ORG_GRADLE_PROJECT_znetLicenseTarget="$rust_target"
# Retain the same explicit selection for the subsequent standalone unit-test
# Gradle invocation, after the Tauri build process has exited.
export ORG_GRADLE_PROJECT_abiList="$expected_abi"
export ORG_GRADLE_PROJECT_archList="$gradle_arch"
export ORG_GRADLE_PROJECT_targetList="$target"
node scripts/install-license-android-hook.mjs
test ! -e "$ANDROID_USER_HOME/debug.keystore"
# Build the packaged frontend, not a dev-server client. Keep the base Tauri/JNI
# namespace; Gradle supplies the separate Android .debug application identity.
pnpm tauri android build --debug --apk --target "$target" --ci --config '{"build":{"beforeBuildCommand":"","devUrl":null}}' 2>&1 | tee android-debug-host-evidence/debug-build.log
test -f "$ANDROID_USER_HOME/debug.keystore"
git diff --exit-code -- src-tauri/Cargo.lock src-tauri/tauri.android.conf.json src-tauri/gen/android/app/build.gradle.kts
(
  cd src-tauri/gen/android
  ./gradlew :app:testUniversalDebugUnitTest --no-daemon
) 2>&1 | tee android-debug-host-evidence/host-tests.log
cp -R src-tauri/gen/android/app/build/test-results android-debug-host-evidence/host-test-results
apk=src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
python3 scripts/verify-debug-android-apk.py "$apk" --build-tools "$tools" --expected-commit "$(git rev-parse HEAD)" --expected-abi "$expected_abi" --receipt android-debug-host-evidence/debug-verification.json
keytool -exportcert -alias androiddebugkey -storepass android -keystore "$ANDROID_USER_HOME/debug.keystore" -file "$state/development-certificate.der" > /dev/null
python3 - "$state/development-certificate.der" android-debug-host-evidence/debug-verification.json <<'PY'
import hashlib, json, pathlib, sys
certificate = hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest()
receipt = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert certificate == receipt['signature']['certificateSha256'], 'APK signer differs from this build development key'
pathlib.Path('android-debug-host-evidence/development-certificate.sha256').write_text(certificate + '\n')
PY
node scripts/license-artifact.mjs --apk "$apk" --profile debug --frontend build/license-inventory.frontend.json --output android-debug-host-evidence/license-inventory.actual.json --receipt android-debug-host-evidence/license-artifact-receipt.json
"$tools/aapt" dump badging "$apk" > android-debug-host-evidence/package-badging.txt
cp "$apk" android-debug-host-evidence/
(cd android-debug-host-evidence && sha256sum app-universal-debug.apk > SHA256SUMS)
# EXIT trap removes the only development keystore, even if any check failed.
