#!/usr/bin/env bash
# Compile the real Tauri Android plugin and JVM tests. No app/APK, signing or device claim.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validation_root="$(mktemp -d "${TMPDIR:-/tmp}/znet-native-io-android.XXXXXX")"
trap 'rm -rf "$validation_root"' EXIT
cargo metadata --manifest-path "$repo_root/src-tauri/Cargo.toml" --format-version 1 --locked > "$validation_root/metadata.json"
python3 - "$repo_root" "$validation_root" <<'PY'
import json, os, pathlib, sys
repo, root = map(pathlib.Path, sys.argv[1:])
metadata = json.loads((root / 'metadata.json').read_text())
tauri = next(p for p in metadata['packages'] if p['name'] == 'tauri')
tauri_android = pathlib.Path(tauri['manifest_path']).parent / 'mobile/android'
def kotlin_string(value):
    return json.dumps(str(value)).replace('$', r'\$')
(root / 'settings.gradle.kts').write_text('''pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories { google(); mavenCentral() } }
rootProject.name = "native-io-validation"
include(":native-io", ":tauri-android")
''' + 'project(":native-io").projectDir = file(' + kotlin_string(repo / 'src-tauri/crates/native-io/android') + ')\n' +
'project(":tauri-android").projectDir = file(' + kotlin_string(tauri_android) + ')\n')
(root / 'build.gradle.kts').write_text('''plugins {
    id("com.android.library") version "8.9.3" apply false
    id("org.jetbrains.kotlin.android") version "1.9.25" apply false
}
subprojects { layout.buildDirectory.set(rootProject.layout.projectDirectory.dir("outputs/${name}")) }
''')
(root / 'gradle.properties').write_text('android.useAndroidX=true\norg.gradle.workers.max=2\norg.gradle.jvmargs=-Xmx1536m\n')
PY
result=0
"${GRADLE_CMD:-gradle}" -p "$validation_root" :native-io:testDebugUnitTest :native-io:assembleDebug --no-daemon || result=$?
if [[ -n "${NATIVE_IO_ARTIFACT_DIR:-}" ]]; then
    mkdir -p "$NATIVE_IO_ARTIFACT_DIR"
    [[ ! -d "$validation_root/outputs/native-io" ]] || cp -R "$validation_root/outputs/native-io" "$NATIVE_IO_ARTIFACT_DIR/android-library"
fi
exit "$result"
