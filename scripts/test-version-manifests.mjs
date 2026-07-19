import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  assertNewerReleaseVersion,
  checkReleaseManifests,
  compareReleaseVersions,
  parseReleaseVersion,
  renderMacInfoPlist,
  setReleaseVersion,
} from "./version-manifests.mjs";

function writeFixture(root, version = "0.0.15") {
  fs.mkdirSync(path.join(root, "src-tauri"), { recursive: true });
  fs.writeFileSync(
    path.join(root, "package.json"),
    `${JSON.stringify({ name: "fixture", version }, null, 2)}\n`,
  );
  fs.writeFileSync(
    path.join(root, "src-tauri", "Cargo.toml"),
    `[package]\nname = "fixture"\nversion = "${version}"\n\n[dependencies]\nother = "0.0.15"\n`,
  );
  fs.writeFileSync(
    path.join(root, "src-tauri", "tauri.conf.json"),
    `${JSON.stringify(
      {
        version,
        identifier: "org.example.fixture",
        bundle: {
          macOS: { bundleVersion: version },
          windows: {
            wix: { version },
            nsis: { installerHooks: "./nsis/uninstall.nsh" },
          },
        },
      },
      null,
      2,
    )}\n`,
  );
  fs.writeFileSync(
    path.join(root, "src-tauri", "Info.plist"),
    renderMacInfoPlist(version),
  );
}

const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "znet-version-test-"));

try {
  writeFixture(fixtureRoot);
  checkReleaseManifests("0.0.15", fixtureRoot);

  const rc = setReleaseVersion("0.0.16-rc.1", fixtureRoot);
  assert.deepEqual(rc, {
    releaseVersion: "0.0.16-rc.1",
    nativeVersion: "0.0.16",
  });
  checkReleaseManifests("0.0.16-rc.1", fixtureRoot);

  const packageJson = JSON.parse(
    fs.readFileSync(path.join(fixtureRoot, "package.json"), "utf8"),
  );
  const tauriConfig = JSON.parse(
    fs.readFileSync(
      path.join(fixtureRoot, "src-tauri", "tauri.conf.json"),
      "utf8",
    ),
  );
  const cargoToml = fs.readFileSync(
    path.join(fixtureRoot, "src-tauri", "Cargo.toml"),
    "utf8",
  );
  const infoPlist = fs.readFileSync(
    path.join(fixtureRoot, "src-tauri", "Info.plist"),
    "utf8",
  );

  assert.equal(packageJson.version, "0.0.16-rc.1");
  assert.match(cargoToml, /^version = "0\.0\.16-rc\.1"$/m);
  assert.match(cargoToml, /^other = "0\.0\.15"$/m);
  assert.equal(tauriConfig.version, "0.0.16-rc.1");
  assert.equal(tauriConfig.bundle.windows.wix.version, "0.0.16");
  assert.equal(tauriConfig.bundle.macOS.bundleVersion, "0.0.16");
  assert.doesNotMatch(infoPlist, /rc\.1/);

  setReleaseVersion("0.0.16", fixtureRoot);
  checkReleaseManifests("0.0.16", fixtureRoot);

  assert.equal(compareReleaseVersions("0.0.16-rc.1", "0.0.15"), 1);
  assert.equal(compareReleaseVersions("0.0.16-rc.2", "0.0.16-rc.1"), 1);
  assert.equal(compareReleaseVersions("0.0.16", "0.0.16-rc.2"), 1);
  assert.equal(compareReleaseVersions("0.0.16-rc.1", "0.0.16-beta.9"), 1);
  assert.equal(compareReleaseVersions("0.0.16-1", "0.0.16-rc.1"), -1);
  assert.equal(compareReleaseVersions("0.0.16-rc.1", "0.0.16-rc.1"), 0);
  assert.throws(
    () => assertNewerReleaseVersion("0.0.16-rc.1", "0.0.16"),
    /must be greater than current version/,
  );
  assert.throws(
    () => setReleaseVersion("0.0.16-rc.1", fixtureRoot),
    /must be greater than current version/,
  );

  for (const invalid of [
    "01.0.0",
    "0.01.0",
    "0.0.01",
    "0.0.16-rc.01",
    "0.0.16-rc..1",
    "256.0.0",
    "0.256.0",
    "0.0.65536",
  ]) {
    assert.throws(() => parseReleaseVersion(invalid), /invalid|MSI limit/);
  }

  const tauriPath = path.join(fixtureRoot, "src-tauri", "tauri.conf.json");
  const inconsistent = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
  inconsistent.bundle.windows.wix.version = "0.0.15";
  fs.writeFileSync(tauriPath, `${JSON.stringify(inconsistent, null, 2)}\n`);
  assert.throws(
    () => checkReleaseManifests("0.0.16", fixtureRoot),
    /release version metadata is inconsistent/,
  );

  console.log("release version manifest tests passed");
} finally {
  fs.rmSync(fixtureRoot, { recursive: true, force: true });
}
