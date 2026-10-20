#!/usr/bin/env bash
# Host JVM only: original policy tests and exact pure JCE helper, no Android SDK/APK.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validation_root="$(mktemp -d "${TMPDIR:-/tmp}/znet-native-policy-jvm.XXXXXX")"
trap 'rm -rf "$validation_root"' EXIT
unset ANDROID_HOME ANDROID_SDK_ROOT ANDROID_NDK_HOME ANDROID_NDK_ROOT NDK_HOME
mkdir -p "$validation_root/src/main/kotlin" "$validation_root/src/test/kotlin"
python3 - "$repo_root" "$validation_root" <<'PY'
import hashlib, json, pathlib, shutil, sys
repo, root = map(pathlib.Path, sys.argv[1:])
android = repo / 'src-tauri/crates/native-io/android/src'
main = android / 'main/java/org/zerodenet/znetsink/nativeio'
tests = android / 'test/java/org/zerodenet/znetsink/nativeio'
shutil.copyfile(main / 'NativeIoPolicy.kt', root / 'src/main/kotlin/NativeIoPolicy.kt')
# Exercise the actual pure helper without compiling NativeVault's Android IO.
# No reimplementation or stubs: changed/ambiguous source boundaries fail closed.
vault = (main / 'NativeVault.kt').read_text()
start_marker = 'internal object NativeVaultEnvelope {'
end_marker = '/** No exported key, raw master file, user-chosen path, or backup of wrapped material. */'
assert vault.count(start_marker) == vault.count(end_marker) == 1
start, end = vault.index(start_marker), vault.index(end_marker)
assert start < end
body = vault[start:end].rstrip()
assert body.endswith('}') and 'android.' not in body
imports = '\n'.join(line for line in vault.splitlines() if line.startswith(('import java.', 'import javax.')))
(root / 'src/main/kotlin/NativeVaultEnvelope.kt').write_text('package org.zerodenet.znetsink.nativeio\n' + imports + '\n' + body + '\n')
for name in ['NativeIoPolicyTest.kt', 'NativeVaultEnvelopeTest.kt']:
    shutil.copyfile(tests / name, root / 'src/test/kotlin' / name)
sources = [main / 'NativeIoPolicy.kt', main / 'NativeVault.kt', tests / 'NativeIoPolicyTest.kt', tests / 'NativeVaultEnvelopeTest.kt']
(root / 'source-receipt.json').write_text(json.dumps(dict(schemaVersion=1, kind='host-jvm-policy',
    androidRuntime=False, sdkUsed=False, packageBuilt=False,
    envelopeBodySha256=hashlib.sha256(body.encode()).hexdigest(),
    sources=[dict(path=str(p.relative_to(repo)), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sources]), indent=2) + '\n')
(root / 'settings.gradle.kts').write_text('''pluginManagement { repositories { gradlePluginPortal(); mavenCentral() } }
dependencyResolutionManagement { repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories { mavenCentral() } }
rootProject.name = "native-policy-host-jvm"
''')
(root / 'build.gradle.kts').write_text('''plugins { kotlin("jvm") version "1.9.25" }
java { sourceCompatibility = JavaVersion.VERSION_1_8; targetCompatibility = JavaVersion.VERSION_1_8 }
tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach { kotlinOptions.jvmTarget = "1.8" }
dependencies { testImplementation("junit:junit:4.13.2") }
tasks.test { useJUnit(); testLogging { events("passed", "failed", "skipped") } }
''')
(root / 'gradle.properties').write_text('org.gradle.workers.max=2\norg.gradle.jvmargs=-Xmx1024m\n')
PY
result=0
"${GRADLE_CMD:-gradle}" -p "$validation_root" test --no-daemon || result=$?
if [[ -n "${NATIVE_IO_ARTIFACT_DIR:-}" ]]; then
    mkdir -p "$NATIVE_IO_ARTIFACT_DIR"
    cp "$validation_root/source-receipt.json" "$NATIVE_IO_ARTIFACT_DIR/"
    for evidence in test-results reports; do
        [[ ! -d "$validation_root/build/$evidence" ]] || cp -R "$validation_root/build/$evidence" "$NATIVE_IO_ARTIFACT_DIR/"
    done
fi
exit "$result"
