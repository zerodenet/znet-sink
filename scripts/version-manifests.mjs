#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const PRERELEASE_IDENTIFIER = '(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)';
const VERSION_PATTERN = new RegExp(`^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(?:-(${PRERELEASE_IDENTIFIER}(?:\\.${PRERELEASE_IDENTIFIER})*))?$`);

const MANIFEST_PATHS = {
  packageJson: 'package.json',
  cargoToml: 'src-tauri/Cargo.toml',
  tauriConfig: 'src-tauri/tauri.conf.json',
  macInfoPlist: 'src-tauri/Info.plist',
};

function fail(message) { throw new Error(message); }

function parseBuildNumber(value) {
  if (value == null || value === '') return null;
  const buildNumber = Number(value);
  if (!Number.isInteger(buildNumber) || buildNumber < 1 || buildNumber > 65535) {
    fail(`native build number '${value}' must be an integer between 1 and 65535`);
  }
  return buildNumber;
}

export function parseReleaseVersion(version) {
  const match = VERSION_PATTERN.exec(version);
  if (!match) fail(`invalid version '${version}'; expected SemVer such as 0.1.0 or 0.1.0-rc.1`);
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);
  if (major > 255) fail(`version major '${major}' exceeds the MSI limit of 255`);
  if (minor > 255) fail(`version minor '${minor}' exceeds the MSI limit of 255`);
  if (patch > 65535) fail(`version patch '${patch}' exceeds the MSI limit of 65535`);
  return { releaseVersion: version, nativeVersion: `${major}.${minor}.${patch}`, prerelease: match[4] ?? null };
}

export function compareReleaseVersions(leftVersion, rightVersion) {
  const left = parseReleaseVersion(leftVersion);
  const right = parseReleaseVersion(rightVersion);
  const leftCore = left.nativeVersion.split('.').map(Number);
  const rightCore = right.nativeVersion.split('.').map(Number);
  for (let i = 0; i < leftCore.length; i += 1) {
    if (leftCore[i] !== rightCore[i]) return leftCore[i] > rightCore[i] ? 1 : -1;
  }
  if (left.prerelease === null || right.prerelease === null) {
    if (left.prerelease === right.prerelease) return 0;
    return left.prerelease === null ? 1 : -1;
  }
  const a = left.prerelease.split('.');
  const b = right.prerelease.split('.');
  const shared = Math.min(a.length, b.length);
  for (let i = 0; i < shared; i += 1) {
    if (a[i] === b[i]) continue;
    const an = /^[0-9]+$/.test(a[i]);
    const bn = /^[0-9]+$/.test(b[i]);
    if (an && bn) {
      if (a[i].length !== b[i].length) return a[i].length > b[i].length ? 1 : -1;
      return a[i] > b[i] ? 1 : -1;
    }
    if (an !== bn) return an ? -1 : 1;
    return a[i] > b[i] ? 1 : -1;
  }
  if (a.length === b.length) return 0;
  return a.length > b.length ? 1 : -1;
}

export function assertNewerReleaseVersion(version, currentVersion) {
  if (compareReleaseVersions(version, currentVersion) <= 0) {
    fail(`version '${version}' must be greater than current version '${currentVersion}'`);
  }
  return parseReleaseVersion(version);
}

function readUtf8(root, relativePath) {
  const fullPath = path.join(root, relativePath);
  if (!fs.existsSync(fullPath)) fail(`expected manifest file not found: ${relativePath}`);
  return fs.readFileSync(fullPath, 'utf8');
}
function writeUtf8(root, relativePath, content) { fs.writeFileSync(path.join(root, relativePath), content, 'utf8'); }
function parseJson(root, relativePath) {
  try { return JSON.parse(readUtf8(root, relativePath)); }
  catch (error) { fail(`failed to parse ${relativePath}: ${error.message}`); }
}
function cargoPackageVersion(cargoToml) {
  let inPackage = false;
  for (const line of cargoToml.split(/\r?\n/)) {
    if (/^\[[^\]]+\]\s*$/.test(line)) { inPackage = line.trim() === '[package]'; continue; }
    if (inPackage) {
      const version = line.match(/^version\s*=\s*"([^"]+)"\s*$/)?.[1];
      if (version) return version;
    }
  }
  fail('could not determine package version from src-tauri/Cargo.toml');
}
function replaceCargoPackageVersion(cargoToml, version) {
  const eol = cargoToml.includes('\r\n') ? '\r\n' : '\n';
  const lines = cargoToml.split(/\r?\n/);
  let inPackage = false;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (/^\[[^\]]+\]\s*$/.test(line)) { inPackage = line.trim() === '[package]'; continue; }
    if (inPackage && /^version\s*=\s*"[^"]+"\s*$/.test(line)) {
      lines[i] = line.replace(/^(version\s*=\s*")[^"]+("\s*)$/, `$1${version}$2`);
      return lines.join(eol);
    }
  }
  fail('could not update package version in src-tauri/Cargo.toml');
}
function plistValue(infoPlist, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return infoPlist.match(new RegExp(`<key>${escapedKey}</key>\\s*<string>([^<]+)</string>`))?.[1];
}

export function renderMacInfoPlist(shortVersion, buildVersion = shortVersion) {
  return `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n  <key>CFBundleShortVersionString</key>\n  <string>${shortVersion}</string>\n  <key>CFBundleVersion</key>\n  <string>${buildVersion}</string>\n</dict>\n</plist>\n`;
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
    macShort: plistValue(infoPlist, 'CFBundleShortVersionString'),
    macBuild: plistValue(infoPlist, 'CFBundleVersion'),
  };
}

function validateFlexibleNative(actual, nativeVersion) {
  const mismatches = [];
  if (actual.macShort !== nativeVersion) mismatches.push(`macShort: expected '${nativeVersion}', found '${actual.macShort ?? '<missing>'}'`);
  const wixMatch = actual.wix?.match(new RegExp(`^${nativeVersion.replaceAll('.', '\\.')}\\.(\\d+)$`));
  if (actual.wix !== nativeVersion && (!wixMatch || Number(wixMatch[1]) > 65535 || Number(wixMatch[1]) < 1)) {
    mismatches.push(`wix: expected '${nativeVersion}' or '${nativeVersion}.<build>', found '${actual.wix ?? '<missing>'}'`);
  }
  const macBuildValid = actual.macBuild === nativeVersion || (/^[1-9]\d*$/.test(actual.macBuild ?? '') && Number(actual.macBuild) <= 65535);
  if (!macBuildValid) mismatches.push(`macBuild: invalid '${actual.macBuild ?? '<missing>'}'`);
  if (actual.macBundle !== actual.macBuild) mismatches.push(`macBundle: expected '${actual.macBuild ?? '<missing>'}', found '${actual.macBundle ?? '<missing>'}'`);
  return mismatches;
}

export function checkReleaseManifests(expectedVersion, root = process.cwd(), buildNumberInput = null) {
  const { nativeVersion } = parseReleaseVersion(expectedVersion);
  const buildNumber = parseBuildNumber(buildNumberInput);
  const actual = readManifestVersions(root);
  const exact = {
    packageJson: expectedVersion,
    cargoToml: expectedVersion,
    tauriConfig: expectedVersion,
  };
  const mismatches = Object.entries(exact)
    .filter(([key, value]) => actual[key] !== value)
    .map(([key, value]) => `${key}: expected '${value}', found '${actual[key] ?? '<missing>'}'`);

  if (buildNumber == null) {
    mismatches.push(...validateFlexibleNative(actual, nativeVersion));
  } else {
    const expected = {
      wix: `${nativeVersion}.${buildNumber}`,
      macBundle: String(buildNumber),
      macShort: nativeVersion,
      macBuild: String(buildNumber),
    };
    mismatches.push(...Object.entries(expected)
      .filter(([key, value]) => actual[key] !== value)
      .map(([key, value]) => `${key}: expected '${value}', found '${actual[key] ?? '<missing>'}'`));
  }
  if (mismatches.length) fail(`release version metadata is inconsistent:\n  ${mismatches.join('\n  ')}`);
  return { releaseVersion: expectedVersion, nativeVersion, buildNumber };
}

export function setReleaseVersion(version, root = process.cwd(), buildNumberInput = null) {
  const packageJson = parseJson(root, MANIFEST_PATHS.packageJson);
  const currentVersion = packageJson.version;
  if (!currentVersion) fail('could not determine current version from package.json');
  checkReleaseManifests(currentVersion, root);
  const { nativeVersion } = assertNewerReleaseVersion(version, currentVersion);
  const buildNumber = parseBuildNumber(buildNumberInput);

  packageJson.version = version;
  writeUtf8(root, MANIFEST_PATHS.packageJson, `${JSON.stringify(packageJson, null, 2)}\n`);
  writeUtf8(root, MANIFEST_PATHS.cargoToml, replaceCargoPackageVersion(readUtf8(root, MANIFEST_PATHS.cargoToml), version));

  const tauriConfig = parseJson(root, MANIFEST_PATHS.tauriConfig);
  tauriConfig.version = version;
  tauriConfig.bundle ??= {};
  tauriConfig.bundle.macOS ??= {};
  tauriConfig.bundle.windows ??= {};
  tauriConfig.bundle.windows.wix ??= {};
  tauriConfig.bundle.macOS.bundleVersion = buildNumber == null ? nativeVersion : String(buildNumber);
  tauriConfig.bundle.windows.wix.version = buildNumber == null ? nativeVersion : `${nativeVersion}.${buildNumber}`;
  writeUtf8(root, MANIFEST_PATHS.tauriConfig, `${JSON.stringify(tauriConfig, null, 2)}\n`);
  writeUtf8(root, MANIFEST_PATHS.macInfoPlist, renderMacInfoPlist(nativeVersion, buildNumber == null ? nativeVersion : String(buildNumber)));

  checkReleaseManifests(version, root, buildNumber);
  return { releaseVersion: version, nativeVersion, buildNumber };
}

function usage() {
  console.log(`Usage:\n  node scripts/version-manifests.mjs validate <version>\n  node scripts/version-manifests.mjs assert-newer <version> <current-version>\n  node scripts/version-manifests.mjs check <version> [build-number]\n  node scripts/version-manifests.mjs set <version> [build-number]`);
}

function runCli() {
  const [command, version, ...extra] = process.argv.slice(2);
  if (!version || !['validate', 'assert-newer', 'check', 'set'].includes(command)) { usage(); process.exitCode = 2; return; }
  let result;
  if (command === 'validate' && extra.length === 0) result = parseReleaseVersion(version);
  else if (command === 'assert-newer' && extra.length === 1) result = assertNewerReleaseVersion(version, extra[0]);
  else if (command === 'check' && extra.length <= 1) result = checkReleaseManifests(version, process.cwd(), extra[0] ?? null);
  else if (command === 'set' && extra.length <= 1) result = setReleaseVersion(version, process.cwd(), extra[0] ?? null);
  else { usage(); process.exitCode = 2; return; }
  console.log(`${command === 'set' ? 'Updated' : 'Verified'} release version ${result.releaseVersion} (native version ${result.nativeVersion}${result.buildNumber ? `, build ${result.buildNumber}` : ''})`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try { runCli(); }
  catch (error) { console.error(`ERROR: ${error.message}`); process.exitCode = 1; }
}
