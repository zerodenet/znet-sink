#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const PRERELEASE_IDENTIFIER =
  "(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)";
const VERSION_PATTERN = new RegExp(
  `^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(?:-(${PRERELEASE_IDENTIFIER}(?:\\.${PRERELEASE_IDENTIFIER})*))?$`,
);

const MANIFEST_PATHS = {
  packageJson: "package.json",
  cargoToml: "src-tauri/Cargo.toml",
  tauriConfig: "src-tauri/tauri.conf.json",
  macInfoPlist: "src-tauri/Info.plist",
};

function fail(message) {
  throw new Error(message);
}

export function parseReleaseVersion(version) {
  const match = VERSION_PATTERN.exec(version);
  if (!match) {
    fail(
      `invalid version '${version}'; expected SemVer such as 0.1.0 or 0.1.0-rc.1`,
    );
  }

  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);

  // WiX ProductVersion is the strictest desktop target. Validate this before
  // creating a release commit so CI cannot fail after the tag is published.
  if (major > 255) {
    fail(`version major '${major}' exceeds the MSI limit of 255`);
  }
  if (minor > 255) {
    fail(`version minor '${minor}' exceeds the MSI limit of 255`);
  }
  if (patch > 65535) {
    fail(`version patch '${patch}' exceeds the MSI limit of 65535`);
  }

  return {
    releaseVersion: version,
    nativeVersion: `${major}.${minor}.${patch}`,
    prerelease: match[4] ?? null,
  };
}

export function compareReleaseVersions(leftVersion, rightVersion) {
  const left = parseReleaseVersion(leftVersion);
  const right = parseReleaseVersion(rightVersion);
  const leftCore = left.nativeVersion.split(".").map(Number);
  const rightCore = right.nativeVersion.split(".").map(Number);

  for (let index = 0; index < leftCore.length; index += 1) {
    if (leftCore[index] !== rightCore[index]) {
      return leftCore[index] > rightCore[index] ? 1 : -1;
    }
  }

  if (left.prerelease === null || right.prerelease === null) {
    if (left.prerelease === right.prerelease) {
      return 0;
    }
    return left.prerelease === null ? 1 : -1;
  }

  const leftIdentifiers = left.prerelease.split(".");
  const rightIdentifiers = right.prerelease.split(".");
  const sharedLength = Math.min(leftIdentifiers.length, rightIdentifiers.length);
  for (let index = 0; index < sharedLength; index += 1) {
    const leftIdentifier = leftIdentifiers[index];
    const rightIdentifier = rightIdentifiers[index];
    if (leftIdentifier === rightIdentifier) {
      continue;
    }

    const leftNumeric = /^[0-9]+$/.test(leftIdentifier);
    const rightNumeric = /^[0-9]+$/.test(rightIdentifier);
    if (leftNumeric && rightNumeric) {
      if (leftIdentifier.length !== rightIdentifier.length) {
        return leftIdentifier.length > rightIdentifier.length ? 1 : -1;
      }
      return leftIdentifier > rightIdentifier ? 1 : -1;
    }
    if (leftNumeric !== rightNumeric) {
      return leftNumeric ? -1 : 1;
    }
    return leftIdentifier > rightIdentifier ? 1 : -1;
  }

  if (leftIdentifiers.length === rightIdentifiers.length) {
    return 0;
  }
  return leftIdentifiers.length > rightIdentifiers.length ? 1 : -1;
}

export function assertNewerReleaseVersion(version, currentVersion) {
  if (compareReleaseVersions(version, currentVersion) <= 0) {
    fail(
      `version '${version}' must be greater than current version '${currentVersion}'; prereleases must target the next unreleased version (for example, 0.0.16-rc.1 after 0.0.15)`,
    );
  }
  return parseReleaseVersion(version);
}

function readUtf8(root, relativePath) {
  const fullPath = path.join(root, relativePath);
  if (!fs.existsSync(fullPath)) {
    fail(`expected manifest file not found: ${relativePath}`);
  }
  return fs.readFileSync(fullPath, "utf8");
}

function writeUtf8(root, relativePath, content) {
  fs.writeFileSync(path.join(root, relativePath), content, "utf8");
}

function parseJson(root, relativePath) {
  try {
    return JSON.parse(readUtf8(root, relativePath));
  } catch (error) {
    fail(`failed to parse ${relativePath}: ${error.message}`);
  }
}

function cargoPackageVersion(cargoToml) {
  let inPackage = false;
  for (const line of cargoToml.split(/\r?\n/)) {
    if (/^\[[^\]]+\]\s*$/.test(line)) {
      inPackage = line.trim() === "[package]";
      continue;
    }
    if (inPackage) {
      const version = line.match(/^version\s*=\s*"([^"]+)"\s*$/)?.[1];
      if (version) {
        return version;
      }
    }
  }
  fail("could not determine package version from src-tauri/Cargo.toml");
}

function replaceCargoPackageVersion(cargoToml, version) {
  const lineEnding = cargoToml.includes("\r\n") ? "\r\n" : "\n";
  const lines = cargoToml.split(/\r?\n/);
  let inPackage = false;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\[[^\]]+\]\s*$/.test(line)) {
      inPackage = line.trim() === "[package]";
      continue;
    }
    if (inPackage && /^version\s*=\s*"[^"]+"\s*$/.test(line)) {
      lines[index] = line.replace(
        /^(version\s*=\s*")[^"]+("\s*)$/,
        `$1${version}$2`,
      );
      return lines.join(lineEnding);
    }
  }
  fail("could not update package version in src-tauri/Cargo.toml");
}

function plistValue(infoPlist, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return infoPlist.match(
    new RegExp(`<key>${escapedKey}</key>\\s*<string>([^<]+)</string>`),
  )?.[1];
}

export function renderMacInfoPlist(nativeVersion) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <!-- Apple bundle versions must contain digits and periods only. -->
  <key>CFBundleShortVersionString</key>
  <string>${nativeVersion}</string>
  <key>CFBundleVersion</key>
  <string>${nativeVersion}</string>
</dict>
</plist>
`;
}

export function readManifestVersions(root = process.cwd()) {
  const packageJson = parseJson(root, MANIFEST_PATHS.packageJson);
  const cargoToml = readUtf8(root, MANIFEST_PATHS.cargoToml);
  const tauriConfig = parseJson(root, MANIFEST_PATHS.tauriConfig);
  const infoPlist = readUtf8(root, MANIFEST_PATHS.macInfoPlist);

  return {
    packageJson: packageJson.version,
    cargoToml: cargoPackageVersion(cargoToml),
    tauriConfig: tauriConfig.version,
    wix: tauriConfig.bundle?.windows?.wix?.version,
    macBundle: tauriConfig.bundle?.macOS?.bundleVersion,
    macShort: plistValue(infoPlist, "CFBundleShortVersionString"),
    macBuild: plistValue(infoPlist, "CFBundleVersion"),
  };
}

export function checkReleaseManifests(expectedVersion, root = process.cwd()) {
  const { nativeVersion } = parseReleaseVersion(expectedVersion);
  const actual = readManifestVersions(root);
  const expected = {
    packageJson: expectedVersion,
    cargoToml: expectedVersion,
    tauriConfig: expectedVersion,
    wix: nativeVersion,
    macBundle: nativeVersion,
    macShort: nativeVersion,
    macBuild: nativeVersion,
  };

  const mismatches = Object.entries(expected)
    .filter(([key, value]) => actual[key] !== value)
    .map(
      ([key, value]) =>
        `${key}: expected '${value}', found '${actual[key] ?? "<missing>"}'`,
    );

  if (mismatches.length > 0) {
    fail(`release version metadata is inconsistent:\n  ${mismatches.join("\n  ")}`);
  }

  return { releaseVersion: expectedVersion, nativeVersion };
}

export function setReleaseVersion(version, root = process.cwd()) {
  const packageJson = parseJson(root, MANIFEST_PATHS.packageJson);
  const currentVersion = packageJson.version;
  if (!currentVersion) {
    fail("could not determine current version from package.json");
  }

  // Refuse to build a new release on top of partially updated manifests.
  checkReleaseManifests(currentVersion, root);
  const { nativeVersion } = assertNewerReleaseVersion(version, currentVersion);

  packageJson.version = version;
  writeUtf8(
    root,
    MANIFEST_PATHS.packageJson,
    `${JSON.stringify(packageJson, null, 2)}\n`,
  );

  const cargoToml = readUtf8(root, MANIFEST_PATHS.cargoToml);
  writeUtf8(
    root,
    MANIFEST_PATHS.cargoToml,
    replaceCargoPackageVersion(cargoToml, version),
  );

  const tauriConfig = parseJson(root, MANIFEST_PATHS.tauriConfig);
  tauriConfig.version = version;
  tauriConfig.bundle ??= {};
  tauriConfig.bundle.macOS ??= {};
  tauriConfig.bundle.macOS.bundleVersion = nativeVersion;
  tauriConfig.bundle.windows ??= {};
  tauriConfig.bundle.windows.wix ??= {};
  tauriConfig.bundle.windows.wix.version = nativeVersion;
  writeUtf8(
    root,
    MANIFEST_PATHS.tauriConfig,
    `${JSON.stringify(tauriConfig, null, 2)}\n`,
  );

  writeUtf8(
    root,
    MANIFEST_PATHS.macInfoPlist,
    renderMacInfoPlist(nativeVersion),
  );

  checkReleaseManifests(version, root);
  return { releaseVersion: version, nativeVersion };
}

function usage() {
  console.log(`Usage:
  node scripts/version-manifests.mjs validate <version>
  node scripts/version-manifests.mjs assert-newer <version> <current-version>
  node scripts/version-manifests.mjs check <version>
  node scripts/version-manifests.mjs set <version>`);
}

function runCli() {
  const [command, version, ...extra] = process.argv.slice(2);
  if (command === "--help" || command === "-h") {
    usage();
    return;
  }
  if (
    !version ||
    !["validate", "assert-newer", "check", "set"].includes(command) ||
    (command === "assert-newer" ? extra.length !== 1 : extra.length > 0)
  ) {
    usage();
    process.exitCode = 2;
    return;
  }

  let result;
  switch (command) {
    case "validate":
      result = parseReleaseVersion(version);
      break;
    case "assert-newer":
      result = assertNewerReleaseVersion(version, extra[0]);
      break;
    case "check":
      result = checkReleaseManifests(version);
      break;
    default:
      result = setReleaseVersion(version);
      break;
  }
  console.log(
    `${command === "set" ? "Updated" : "Verified"} release version ${result.releaseVersion} (native package version ${result.nativeVersion})`,
  );
}

if (
  process.argv[1] &&
  pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url
) {
  try {
    runCli();
  } catch (error) {
    console.error(`ERROR: ${error.message}`);
    process.exitCode = 1;
  }
}
