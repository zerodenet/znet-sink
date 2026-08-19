import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { assertNewerReleaseVersion, checkReleaseManifests, compareReleaseVersions, parseReleaseVersion, renderMacInfoPlist, setReleaseVersion } from './version-manifests.mjs';

function writeFixture(root, version = '0.0.15') {
  fs.mkdirSync(path.join(root, 'src-tauri'), { recursive: true });
  fs.writeFileSync(path.join(root, 'package.json'), `${JSON.stringify({ name: 'fixture', version }, null, 2)}\n`);
  fs.writeFileSync(path.join(root, 'src-tauri', 'Cargo.toml'), `[package]\nname = "fixture"\nversion = "${version}"\n\n[dependencies]\nother = "0.0.15"\n`);
  fs.writeFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), `${JSON.stringify({ version, identifier: 'org.example.fixture', bundle: { macOS: { bundleVersion: version }, windows: { wix: { version }, nsis: { installerHooks: './nsis/uninstall.nsh' } } } }, null, 2)}\n`);
  fs.writeFileSync(path.join(root, 'src-tauri', 'Info.plist'), renderMacInfoPlist(version));
}

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'znet-version-test-'));
try {
  writeFixture(root);
  checkReleaseManifests('0.0.15', root);
  const rc = setReleaseVersion('0.0.16-rc.202608181036', root, 101);
  assert.deepEqual(rc, { releaseVersion: '0.0.16-rc.202608181036', nativeVersion: '0.0.16', buildNumber: 101 });
  checkReleaseManifests('0.0.16-rc.202608181036', root, 101);

  const packageJson = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const tauri = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'));
  const cargo = fs.readFileSync(path.join(root, 'src-tauri', 'Cargo.toml'), 'utf8');
  const plist = fs.readFileSync(path.join(root, 'src-tauri', 'Info.plist'), 'utf8');
  assert.equal(packageJson.version, '0.0.16-rc.202608181036');
  assert.match(cargo, /^version = "0\.0\.16-rc\.202608181036"$/m);
  assert.match(cargo, /^other = "0\.0\.15"$/m);
  assert.equal(tauri.version, '0.0.16-rc.202608181036');
  assert.equal(tauri.bundle.windows.wix.version, '0.0.16.101');
  assert.equal(tauri.bundle.macOS.bundleVersion, '101');
  assert.match(plist, /<key>CFBundleShortVersionString<\/key>\s*<string>0\.0\.16<\/string>/);
  assert.match(plist, /<key>CFBundleVersion<\/key>\s*<string>101<\/string>/);

  setReleaseVersion('0.0.16', root, 102);
  checkReleaseManifests('0.0.16', root, 102);
  assert.equal(compareReleaseVersions('0.0.16-rc.2', '0.0.16-rc.1'), 1);
  assert.equal(compareReleaseVersions('0.0.16', '0.0.16-rc.2'), 1);
  assert.throws(() => assertNewerReleaseVersion('0.0.16-rc.1', '0.0.16'), /must be greater/);
  assert.throws(() => setReleaseVersion('0.0.16-rc.1', root, 103), /must be greater/);
  assert.throws(() => setReleaseVersion('0.0.17-dev.202608181037', root, 65536), /native build number/);
  for (const invalid of ['01.0.0', '0.01.0', '0.0.01', '0.0.16-rc.01', '0.0.16-rc..1', '256.0.0', '0.256.0', '0.0.65536']) {
    assert.throws(() => parseReleaseVersion(invalid), /invalid|MSI limit/);
  }
  console.log('release version manifest tests passed');
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
