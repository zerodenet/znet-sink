import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
const read = file => readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
test('locked CLI and standard generator retain main app, UI and native plugin ownership', () => {
  assert.match(read('pnpm-lock.yaml'), /'@tauri-apps\/cli@2\.11\.4'/);
  const init = read('scripts/ios-host.mjs');
  assert.match(init, /'ios', 'init', '--ci', '--skip-targets-install'/);
  assert.match(init, /process\.platform !== 'darwin'/);
  assert.match(read('src-tauri/src/mobile_host/application.rs'), /tauri_plugin_znet_native_io::init/);
  assert.match(read('src-tauri/src/lib.rs'), /tauri::mobile_entry_point/);
  // Tauri supplies CARGO_PKG_NAME to SwiftLinker; a standalone Xcode scheme
  // can compile successfully while exporting a differently named archive.
  const cargoName = read('src-tauri/crates/native-io/Cargo.toml').match(/^name = "([^"]+)"/m)[1];
  const swiftPackage = read('src-tauri/crates/native-io/ios/Package.swift');
  assert.ok(swiftPackage.includes(`.library(name: "${cargoName}", type: .static`));
  assert.ok(swiftPackage.includes(`let package = Package(\n    name: "${cargoName}"`));
});
test('full build uses no-sign, embedded frontend and separate simulator/device evidence', () => {
  const build = read('scripts/build-ios-host.sh');
  assert.match(build, /--ci --no-sign/);
  assert.match(build, /"devUrl":null/);
  assert.match(build, /cli_target=aarch64-sim/);
  assert.match(build, /rust_target=aarch64-apple-ios; cli_target=aarch64/);
  assert.match(build, /Refusing signing environment/);
  assert.match(build, /export CARGO_PROFILE_DEV_DEBUG=1/);
  assert.match(read('.github/workflows/ios-app-host.yml'), /target: \[simulator, device\]/);
  assert.equal(JSON.parse(read('src-tauri/tauri.ios.conf.json')).bundle.iOS.minimumSystemVersion, '16.4');
});
test('smoke is Debug and launch-nonce gated; real UI/IPC retains unsupported VPN and license gap', () => {
  const swift = read('src-tauri/crates/native-io/ios/Sources/NativeIoPlugin/HostSmoke.swift');
  assert.match(swift, /#if DEBUG/); assert.match(swift, /nonce\.count == 32/);
  assert.match(swift, /Bundle\.main\.url\(forResource: "ios-host-smoke"/);
  const driver = read('src-tauri/apple/ios-host-smoke.js');
  assert.match(driver, /invoke\('gui_mobile_platform_snapshot'\)/);
  assert.match(driver, /!platform\.vpnSupported/);
  assert.match(driver, /licenses\.native\.status === 'unavailable'/);
  assert.doesNotMatch(driver, /localStorage|mockResolvedValue|__TAURI_IPC__/);
  assert.match(read('src-tauri/crates/native-io/src/lib.rs'), /return Err\(Error::Unavailable\)/);
});
test('actual verifier negative contracts reject stale receipts, wrong SDK and absent native UI', () => {
  execFileSync('python3', ['-B', 'scripts/test-ios-host.py'], { stdio: 'pipe' });
});
test('Linux cannot silently generate a fake Xcode project', { skip: process.platform === 'darwin' }, () => {
  const run = spawnSync(process.execPath, ['scripts/ios-host.mjs', 'init'], { encoding: 'utf8' });
  assert.notEqual(run.status, 0); assert.match(run.stderr, /requires macOS\/Xcode/);
});
