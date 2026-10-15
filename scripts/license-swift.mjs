#!/usr/bin/env node
// Inventory observed Swift package sources and unsigned library inputs; never infer app bundling.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { applyPackageMaterialCache } from './license-materials.mjs';
import { sha256 as hash, entryId as commonEntryId, readTextMaterials, serializeInventory } from './license-common.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const nativeRoot = path.join(repo, 'src-tauri/crates/native-io/ios');
const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index];
  if (!key?.startsWith('--') || !process.argv[index + 1]) throw new Error('Expected --option value');
  args.set(key.slice(2), process.argv[index + 1]);
}
const allowed = new Set(['cargo-metadata', 'swift-build-path', 'target', 'output', 'build-log',
  'sdk-data', 'sdk-path', 'artifact', 'resolved-file', 'profile', 'cargo-lock', 'cargo-archive', 'materials-cache']);
for (const key of args.keys()) if (!allowed.has(key)) throw new Error(`Unknown option: ${key}`);
if (!args.has('cargo-metadata') || !args.has('output')) {
  throw new Error('Required: --cargo-metadata PATH --output PATH');
}
const triple = args.get('target') ?? 'aarch64-apple-ios-sim';
if (!['aarch64-apple-ios', 'aarch64-apple-ios-sim', 'x86_64-apple-ios'].includes(triple)) {
  throw new Error('Expected an iOS target triple');
}
let inventory = {
  schemaVersion: 1,
  target: { platform: 'ios', triple, profile: args.get('profile') ?? 'unsigned-native-library', features: [] },
  provenance: { kind: 'swift-source-and-unsigned-library', inputs: [] },
  entries: [], texts: [], gaps: []
};
const gap = (reason, id) => inventory.gaps.push({ ecosystem: 'swift', ...(id ? { id } : {}), reason });
const input = (file, label) => {
  const bytes = fs.readFileSync(file);
  inventory.provenance.inputs.push({ label, sha256: hash(bytes) });
  return bytes;
};
const entryId = (name, version) => commonEntryId('swift', name, version);
const textIds = new Map();
function addText(text, name, kind, source) {
  const sha256 = hash(text);
  const id = `sha256-${sha256}`;
  if (!textIds.has(id)) {
    textIds.set(id, true);
    inventory.texts.push({ id, text, name, kind, source, sha256 });
  }
  return id;
}
function materials(directory, sourceLabel) {
  if (directory !== repo) {
    const result = readTextMaterials(directory, sourceLabel);
    for (const text of result.texts) {
      if (!textIds.has(text.id)) { textIds.set(text.id, true); inventory.texts.push(text); }
    }
    for (const issue of result.gaps) gap(`${issue.reason}: ${issue.source}`);
    return { licenseTextIds: result.texts.filter((text) => text.kind === 'license').map((text) => text.id),
      noticeTextIds: result.texts.filter((text) => text.kind !== 'license').map((text) => text.id) };
  }
  const licenses = [], notices = [];
  for (const name of fs.readdirSync(directory).sort()) {
    const file = path.join(directory, name);
    if (!fs.statSync(file).isFile()) continue;
    const isLicense = /^licen[sc]e(?:$|[._-])/i.test(name);
    const isNotice = /^(?:notice|copyright)(?:$|[._-])/i.test(name);
    if (!isLicense && !isNotice) continue;
    const text = fs.readFileSync(file, 'utf8');
    const id = addText(text, name, isLicense ? 'license' : 'notice', `${sourceLabel}/${name}`);
    (isLicense ? licenses : notices).push(id);
  }
  return { licenseTextIds: licenses, noticeTextIds: notices };
}
function swiftFiles(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((item) => item.isDirectory() ? swiftFiles(path.join(directory, item.name))
      : item.name.endsWith('.swift') ? [path.join(directory, item.name)] : []);
}
const imports = new Map();
function sourceEvidence(root, label) {
  const manifest = fs.readFileSync(path.join(root, 'Package.swift'), 'utf8');
  const sourceRoots = [path.join(root, 'Sources')];
  for (const match of manifest.matchAll(/\.target\(\s*name:\s*"[^"]+"[\s\S]*?path:\s*"([^"]+)"/g)) {
    const targetPath = path.resolve(root, match[1]);
    if (!targetPath.startsWith(`${root}${path.sep}`)) throw new Error('Swift target path escapes the pinned package');
    sourceRoots.push(targetPath);
  }
  const files = [...new Set(sourceRoots.flatMap(swiftFiles))].sort();
  if (!files.length) gap(`No declared production Swift source files observed for ${label}.`);
  for (const file of files) {
    const relative = path.relative(root, file).split(path.sep).join('/');
    const source = input(file, `${label}/${relative}`).toString('utf8');
    for (const match of source.matchAll(/^\s*(?:@_exported\s+)?import\s+([\w.]+)/gm)) {
      const module = match[1];
      const names = imports.get(module) ?? [];
      names.push(`${label}/${relative}`);
      imports.set(module, names);
    }
  }
  return files;
}
function observedHeaders(files, root, sourceLabel) {
  const ids = [];
  for (const file of files) {
    const source = fs.readFileSync(file, 'utf8');
    // Preserve only actual upstream copyright/SPDX header lines, never infer a holder.
    const lines = source.split(/\r?\n/).filter((line) => /^\s*\/\/\s*(?:Copyright\b|SPDX-License-Identifier:)/i.test(line));
    if (lines.length) ids.push(addText(`${lines.join('\n')}\n`, path.basename(file), 'copyright',
      `${sourceLabel}/${path.relative(root, file).split(path.sep).join('/')}`));
  }
  return [...new Set(ids)];
}
const metadata = JSON.parse(input(args.get('cargo-metadata'), 'locked-cargo-metadata').toString('utf8'));
const host = metadata.packages.find((pkg) => pkg.name === 'tauri-plugin-znet-native-io');
if (!host) throw new Error('Cargo metadata must include tauri-plugin-znet-native-io');
const node = metadata.resolve?.nodes.find((item) => item.id === host.id);
const tauriId = node?.deps.find((dep) => dep.name === 'tauri')?.pkg;
const tauri = metadata.packages.find((pkg) => pkg.id === tauriId);
if (!tauri) throw new Error('Cargo metadata must resolve the native IO package direct Tauri dependency');
const cargoLock = input(args.get('cargo-lock') ?? path.join(repo, 'src-tauri/Cargo.lock'), 'locked-Cargo.lock').toString('utf8');
const lockedTauri = cargoLock.split('[[package]]').map((block) => {
  const field = (name) => block.match(new RegExp(`^${name} = "([^"\\r\\n]+)"`, 'm'))?.[1];
  return { name: field('name'), version: field('version'), source: field('source'), checksum: field('checksum') };
}).find((pkg) => pkg.name === 'tauri' && pkg.version === tauri.version && pkg.source === tauri.source);
if (!lockedTauri || !/^[a-f0-9]{64}$/.test(lockedTauri.checksum ?? '')) {
  throw new Error('Resolved Tauri metadata is not bound to a checksummed Cargo.lock registry package');
}
const tauriRoot = path.dirname(tauri.manifest_path);
const registryRoot = path.resolve(tauriRoot, '../../..');
const crateArchive = args.get('cargo-archive') ?? path.join(registryRoot, 'cache', path.basename(path.dirname(tauriRoot)), `tauri-${tauri.version}.crate`);
let verifiedTauriArchive = false;
if (fs.existsSync(crateArchive)) {
  const bytes = input(crateArchive, `locked-cargo-archive:tauri@${tauri.version}`);
  if (hash(bytes) !== lockedTauri.checksum) throw new Error('Tauri Cargo archive does not match the locked checksum');
  verifiedTauriArchive = true;
} else gap('Locked Tauri Cargo archive unavailable; extracted Swift/legal files cannot be checked against the registry checksum.');
function verifyTauriFiles(files) {
  if (!verifiedTauriArchive) return;
  for (const file of files) {
    const relative = path.relative(tauriRoot, file).split(path.sep).join('/');
    const archived = execFileSync('tar', ['-xOf', crateArchive, `tauri-${tauri.version}/${relative}`], { maxBuffer: 2 * 1024 * 1024, stdio: ['ignore', 'pipe', 'ignore'] });
    if (!archived.equals(fs.readFileSync(file))) throw new Error('Tauri Swift/legal source differs from the locked Cargo archive');
  }
}
const tauriApi = path.join(tauriRoot, 'mobile/ios-api');
if (!fs.existsSync(path.join(tauriApi, 'Package.swift'))) throw new Error('Resolved Cargo Tauri source lacks ios-api');
const nativeManifest = input(path.join(nativeRoot, 'Package.swift'), 'native-io/Package.swift').toString('utf8');
if (!nativeManifest.includes('.package(name: "Tauri", path: "../.tauri/tauri-api")')) {
  throw new Error('NativeIO Tauri package declaration changed; review inventory mapping');
}
const nativeFiles = sourceEvidence(nativeRoot, 'native-io');
const tauriLabel = `cargo:tauri@${tauri.version}/mobile/ios-api`;
const tauriManifest = input(path.join(tauriApi, 'Package.swift'), `${tauriLabel}/Package.swift`).toString('utf8');
const tauriFiles = sourceEvidence(tauriApi, tauriLabel);
inventory.entries.push({
  id: entryId('znet-native-io', host.version), ecosystem: 'swift', name: 'znet-native-io',
  version: host.version, declaredLicense: host.license ?? null, source: 'repository:src-tauri/crates/native-io/ios',
  evidence: [{ kind: 'build-input', detail: 'Repository Swift source package; version and license from owning Cargo package. Apple builds include NativeIoPolicy and znet-native-io; Linux builds include only Foundation policy/tests.' }],
  ...materials(repo, 'repository'), textStatus: 'available', missing: []
});
const tauriTexts = materials(tauriRoot, `cargo:tauri@${tauri.version}`);
verifyTauriFiles([path.join(tauriApi, 'Package.swift'), ...tauriFiles,
  ...inventory.texts.filter((text) => text.source.startsWith(`cargo:tauri@${tauri.version}/`))
    .map((text) => path.join(tauriRoot, text.source.slice(`cargo:tauri@${tauri.version}/`.length)))]);
tauriTexts.noticeTextIds.push(...observedHeaders(tauriFiles.concat(path.join(tauriApi, 'Package.swift')), tauriApi, tauriLabel));
inventory.entries.push({
  id: entryId('Tauri', tauri.version), ecosystem: 'swift', name: 'Tauri', version: tauri.version,
  declaredLicense: tauri.license ?? null, source: `${tauri.source ?? 'cargo-local'}#tauri@${tauri.version}/mobile/ios-api`,
  evidence: [{ kind: 'resolved-target', detail: `Original Cargo source matches the locked registry identity tauri@${tauri.version}, checksum ${lockedTauri.checksum}; ${verifiedTauriArchive ? 'consumed Swift and legal bytes match that archive' : 'archive byte comparison unavailable'}.` },
    { kind: 'build-input', detail: 'NativeIO declares the generated local Tauri Swift package dependency.' }],
  ...tauriTexts, textStatus: tauriTexts.licenseTextIds.length ? 'available' : 'missing',
  missing: tauriTexts.licenseTextIds.length ? [] : ['Actual Tauri license text unavailable']
});

const remoteDeclarations = [...tauriManifest.matchAll(/\.package\(name:\s*"([^"]+)",\s*url:\s*"([^"]+)",\s*from:\s*"([^"]+)"\)/g)];
if (remoteDeclarations.length !== 1 || remoteDeclarations[0][1] !== 'SwiftRs') {
  throw new Error('Tauri remote Swift dependencies changed; review inventory traversal');
}
const buildRoot = args.get('swift-build-path');
const candidateLocks = [args.get('resolved-file'), buildRoot && path.join(buildRoot, 'Package.resolved'),
  buildRoot && path.join(buildRoot, 'SourcePackages/Package.resolved'), path.join(nativeRoot, 'Package.resolved')].filter(Boolean);
const lockPath = candidateLocks.find((file) => fs.existsSync(file));
let pins = [];
if (lockPath) {
  const lock = JSON.parse(input(lockPath, 'actual-native-build/Package.resolved').toString('utf8'));
  pins = lock.pins ?? lock.object?.pins ?? [];
} else {
  gap('No Package.resolved for the actual NativeIO Apple build. Swift remote versions remain unresolved; unrelated plugin locks and Rust swift-rs versions are not substitutes.');
}
const stateFile = buildRoot && [path.join(buildRoot, 'workspace-state.json'), path.join(buildRoot, 'SourcePackages/workspace-state.json')]
  .find((file) => fs.existsSync(file));
const state = stateFile ? JSON.parse(input(stateFile, 'actual-native-build/workspace-state.json').toString('utf8')) : null;
const dependencies = state?.object?.dependencies ?? [];
for (const [, name, url, constraint] of remoteDeclarations) {
  const pin = pins.find((item) => (item.location ?? item.repositoryURL)?.replace(/\.git$/, '').toLowerCase() === url.toLowerCase());
  const revision = typeof pin?.state?.revision === 'string' && /^[a-f0-9]{40}$/.test(pin.state.revision) ? pin.state.revision : null;
  const version = pin?.state?.version ?? (revision ? `revision-${revision}` : 'unresolved');
  const id = entryId(name, version);
  const entry = { id, ecosystem: 'swift', name, version, declaredLicense: null, source: revision ? `${url}#${revision}` : url,
    manifestSha256: hash(tauriManifest), checkoutRevision: revision,
    evidence: [{ kind: 'build-input', detail: `Tauri Package.swift declares ${name} from ${constraint}; a lower bound is not an exact version.` }],
    licenseTextIds: [], noticeTextIds: [], textStatus: 'missing', missing: [] };
  if (pin && revision) entry.evidence.push({ kind: 'resolved-target', detail: `Actual NativeIO Package.resolved pins ${name} to ${version}, revision ${revision}.` });
  else entry.missing.push('Exact Swift package pin is unavailable');
  const dependency = dependencies.find((item) => (item.packageRef?.location ?? '').replace(/\.git$/, '').toLowerCase() === url.toLowerCase());
  const checkoutsDirectory = stateFile && path.resolve(path.dirname(stateFile), 'checkouts');
  const checkoutRoot = checkoutsDirectory && dependency?.subpath && path.resolve(checkoutsDirectory, dependency.subpath);
  if (checkoutRoot && !checkoutRoot.startsWith(`${checkoutsDirectory}${path.sep}`)) {
    throw new Error('SwiftPM checkout path escapes the actual build checkout directory');
  }
  if (revision && checkoutRoot && fs.existsSync(checkoutRoot)) {
    let checkoutRevision;
    try { checkoutRevision = execFileSync('git', ['-C', checkoutRoot, 'rev-parse', 'HEAD'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim(); } catch { /* no unchecked source claims */ }
    const checkoutState = dependency?.state?.checkoutState;
    if (checkoutState?.revision !== revision || (pin.state.version && checkoutState.version !== pin.state.version)) {
      entry.missing.push('SwiftPM workspace checkout state does not match the actual resolved pin');
    } else if (checkoutRevision === revision) {
      // HEAD alone is insufficient: every consumed file must be a tracked blob at
      // this commit. A dirty worktree is not original revision evidence.
      const tracked = execFileSync('git', ['-C', checkoutRoot, 'ls-files', '-z'], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024 }).split('\0').filter(Boolean);
      const verifyCheckoutFile = (file) => {
        const relative = path.relative(checkoutRoot, file).split(path.sep).join('/');
        if (!tracked.includes(relative) || !execFileSync('git', ['-C', checkoutRoot, 'show', `${revision}:${relative}`], { maxBuffer: 2 * 1024 * 1024, stdio: ['ignore', 'pipe', 'ignore'] }).equals(fs.readFileSync(file))) {
          throw new Error('Consumed Swift source/legal material differs from its pinned Git revision');
        }
      };
      const label = `swift:${name}@${revision}`;
      const packageFile = path.join(checkoutRoot, 'Package.swift');
      if (!fs.existsSync(packageFile)) throw new Error('Resolved Swift package checkout has no Package.swift');
      input(packageFile, `${label}/Package.swift`);
      const files = sourceEvidence(checkoutRoot, label);
      const manifestSource = fs.readFileSync(packageFile, 'utf8');
      if (/\.package\([^)]*url\s*:/.test(manifestSource.replace(/^\s*\/\/.*$/gm, ''))) {
        gap(`Additional remote Swift dependencies declared by pinned ${name}; recursive resolution is not yet supported.`, id);
      }
      const records = materials(checkoutRoot, label);
      const materialFiles = inventory.texts.filter((text) => text.source.startsWith(`${label}/`))
        .map((text) => path.join(checkoutRoot, text.source.slice(`${label}/`.length)));
      for (const file of [packageFile, ...files, ...materialFiles]) verifyCheckoutFile(file);
      entry.licenseTextIds = records.licenseTextIds;
      entry.noticeTextIds = records.noticeTextIds.concat(observedHeaders(files, checkoutRoot, label));
      const spdx = new Set(files.flatMap((file) => [...fs.readFileSync(file, 'utf8').matchAll(/SPDX-License-Identifier:\s*([^\r\n]+)/g)].map((match) => match[1].trim())));
      if (spdx.size === 1) entry.declaredLicense = [...spdx][0];
      if (spdx.size > 1) entry.missing.push('Multiple distinct source SPDX declarations require review; no package license expression inferred');
      entry.textStatus = entry.licenseTextIds.length ? 'available' : 'missing';
      entry.evidence.push({ kind: 'resolved-target', detail: 'SwiftPM checkout Git HEAD matches the actual Package.resolved revision; license/notice/source evidence comes from that checkout.' });
      if (!entry.licenseTextIds.length) entry.missing.push('Pinned checkout provides no observed root license text');
      if (!entry.declaredLicense) entry.missing.push('Pinned Swift source does not provide a single package-level SPDX declaration; actual license texts remain available for review');
    } else entry.missing.push('SwiftPM checkout Git HEAD does not match the resolved pin');
  } else entry.missing.push('Verified source checkout for the actual Swift pin is unavailable');
  inventory.entries.push(entry);
  for (const reason of entry.missing) gap(reason, id);
}

let sdkPath = args.get('sdk-path');
if (!sdkPath && process.platform === 'darwin') {
  try { sdkPath = execFileSync('xcrun', ['--sdk', triple === 'aarch64-apple-ios' ? 'iphoneos' : 'iphonesimulator', '--show-sdk-path'], { encoding: 'utf8' }).trim(); } catch { /* recorded below */ }
}
const sdkFile = args.get('sdk-data') ?? (sdkPath && path.join(sdkPath, 'SDKSettings.json'));
let sdk = null;
if (sdkFile && fs.existsSync(sdkFile)) sdk = JSON.parse(input(sdkFile, 'apple-sdk/SDKSettings.json').toString('utf8'));
if (!sdk) inventory.gaps.push({ ecosystem: 'platform', reason: 'Apple SDK metadata unavailable; framework entries describe source imports, not observed SDK linking or redistribution licenses.' });
else if (!sdkPath) inventory.gaps.push({ ecosystem: 'platform', reason: 'SDKSettings metadata observed without an SDK root; individual framework/module metadata was not inspected.' });
if (sdk) {
  const expectedSdk = triple === 'aarch64-apple-ios' ? 'iphoneos' : 'iphonesimulator';
  if (typeof sdk.CanonicalName !== 'string' || !new RegExp(`^${expectedSdk}(?:[0-9.]*)$`).test(sdk.CanonicalName)
      || typeof sdk.Version !== 'string' || !/^[0-9]+(?:\.[0-9]+)*$/.test(sdk.Version)) {
    throw new Error('Observed Apple SDK identity does not match the requested iOS device/simulator target');
  }
}
const sdkName = sdk?.CanonicalName ?? sdk?.DisplayName ?? 'unobserved-apple-sdk';
const sdkVersion = sdk?.Version ?? 'unobserved';
const localModules = new Set(['Tauri', 'NativeIoPolicy', 'SwiftRs']);
for (const [name, files] of [...imports.entries()].sort(([left], [right]) => left.localeCompare(right))) {
  if (localModules.has(name)) continue;
  const evidence = [{ kind: 'runtime-provided', detail: `Declared source import ${name}; intended Apple platform module, not a bundled Swift package or proof of app linkage.` },
    { kind: 'build-input', detail: `Imported by ${[...new Set(files)].sort().join(', ')}` }];
  const framework = sdkPath && path.join(sdkPath, 'System/Library/Frameworks', `${name.split('.')[0]}.framework`);
  if (framework && fs.existsSync(framework)) {
    for (const relative of ['Info.plist', `${name.split('.')[0]}.tbd`, 'Modules/module.modulemap']) {
      const file = path.join(framework, relative);
      if (fs.existsSync(file)) {
        const metadataText = input(file, `apple-sdk/${name.split('.')[0]}.framework/${relative}`).toString('utf8');
        let identifier;
        if (relative.endsWith('.tbd')) {
          const install = metadataText.match(/^\s*install-name:\s*['"]?([^'"\r\n]+)['"]?\s*$/m)?.[1]?.trim();
          if (install) identifier = path.posix.basename(install);
        } else if (relative === 'Info.plist') {
          identifier = metadataText.match(/<key>CFBundleIdentifier<\/key>\s*<string>([^<]+)<\/string>/)?.[1];
          if (!identifier && process.platform === 'darwin') {
            try {
              const info = JSON.parse(execFileSync('plutil', ['-convert', 'json', '-o', '-', file], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }));
              identifier = info.CFBundleIdentifier;
            } catch { /* hashed opaque SDK metadata remains useful without guessed fields */ }
          }
        }
        const safeIdentifier = typeof identifier === 'string' && /^[A-Za-z0-9._-]+$/.test(identifier) ? `; observed identifier ${identifier}` : '';
        evidence.push({ kind: 'build-input', detail: `Observed ${sdkName} ${sdkVersion} framework metadata ${name.split('.')[0]}.framework/${relative}${safeIdentifier}; SDK version is not a runtime framework version, and metadata does not assert a redistribution obligation.` });
      }
    }
  }
  inventory.entries.push({ id: commonEntryId('platform', name, String(sdkVersion), `apple-sdk:${sdkName}`), ecosystem: 'platform', name,
    version: String(sdkVersion), declaredLicense: null, source: `apple-sdk:${sdkName}`,
    evidence, licenseTextIds: [], noticeTextIds: [], textStatus: 'not-applicable', missing: [] });
}

let log = null;
if (args.has('build-log')) log = input(args.get('build-log'), 'unsigned-ios-library/build-log').toString('utf8');
const appleArch = triple.startsWith('aarch64') ? 'arm64' : 'x86_64';
const sdkExpression = triple === 'aarch64-apple-ios' ? /SDKROOT\s*=\s*iphoneos|\-sdk\s+iphoneos\b|\-target\s+arm64-apple-ios[\d.]+(?:\s|$)/ : /SDKROOT\s*=\s*iphonesimulator|\-sdk\s+iphonesimulator\b|\-target\s+(?:arm64|x86_64)-apple-ios[\d.]+-simulator/;
const compiledContext = Boolean(log && /\*\* BUILD SUCCEEDED \*\*/.test(log) && sdkExpression.test(log)
  && new RegExp(`(?:SwiftCompile|SwiftDriver Compilation)\\b[^\\r\\n]*?\\bnormal\\s+${appleArch}\\b`).test(log));
if (compiledContext) {
  for (const entry of inventory.entries.filter((item) => item.ecosystem === 'swift')) {
    const module = entry.name === 'znet-native-io' ? 'znet_native_io' : entry.name;
    if (log.includes(`-module-name ${module}`) || log.includes(`/Sources/${module}/`)) {
      entry.evidence.push({ kind: 'compiled', detail: `Successful unsigned Xcode build log observes Swift module ${module} for ${triple}; this is library compilation, not application bundle inclusion.` });
    }
  }
} else gap('No validated successful Xcode build log for this iOS target. Foundation-only Linux tests and Swift syntax parsing do not prove UIKit library compilation.');
if (args.has('artifact')) {
  const file = args.get('artifact');
  const bytes = fs.readFileSync(file);
  if (bytes.subarray(0, 8).toString('ascii') !== '!<arch>\n') throw new Error('Unsigned Swift library artifact must be an ar archive; app bundle verification is outside this inventory');
  let members;
  try { members = execFileSync('ar', ['-t', file], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim().split('\n').filter(Boolean); }
  catch { throw new Error('Swift library archive member table could not be validated'); }
  const object = members.find((name) => name.endsWith('.o'));
  if (!object) throw new Error('Swift library archive contains no observed object member');
  const witness = execFileSync('ar', ['-p', file, object], { maxBuffer: 64 * 1024 * 1024, stdio: ['ignore', 'pipe', 'ignore'] });
  if (witness.length < 32 || witness.readUInt32LE(0) !== 0xfeedfacf) {
    throw new Error('Swift library archive object is not observed 64-bit Mach-O');
  }
  const expectedCpu = appleArch === 'arm64' ? 0x0100000c : 0x01000007;
  if (witness.readUInt32LE(4) !== expectedCpu) throw new Error('Swift library object architecture does not match requested target');
  let platform;
  let cursor = 32;
  for (let index = 0; index < witness.readUInt32LE(16); index++) {
    if (cursor + 8 > witness.length) throw new Error('Swift library object load commands are truncated');
    const command = witness.readUInt32LE(cursor), size = witness.readUInt32LE(cursor + 4);
    if (size < 8 || cursor + size > witness.length) throw new Error('Swift library object load command size is invalid');
    if (command === 0x32 && size >= 24) platform = witness.readUInt32LE(cursor + 8);
    cursor += size;
  }
  const expectedPlatform = triple === 'aarch64-apple-ios' ? 2 : 7;
  if (platform !== expectedPlatform) throw new Error('Swift library object LC_BUILD_VERSION does not establish requested iOS device/simulator target');
  inventory.provenance.artifact = { name: path.basename(file), sha256: hash(bytes) };
  if (!compiledContext) gap('Archive artifact observed, but its iOS target compilation is not validated by the supplied build log.');
} else gap('No unsigned Swift compiled-library archive supplied. Declared dependencies are not reported as bundled.');
gap('No generated or verified iOS application host/provider artifact is covered. This inventory does not prove app bundling, SDK framework linkage, signing, or device acceptance.');
const cacheFile = args.get('materials-cache') ?? path.join(repo, 'resources/licenses/swift-package-materials.json');
if (fs.existsSync(cacheFile)) {
  const cache = JSON.parse(input(cacheFile, 'resources/licenses/swift-package-materials.json').toString('utf8'));
  inventory = applyPackageMaterialCache(inventory, cache);
}
for (const entry of inventory.entries) {
  entry.licenseTextIds = [...new Set(entry.licenseTextIds)].sort();
  entry.noticeTextIds = [...new Set(entry.noticeTextIds)].sort();
  entry.evidence.sort((a, b) => `${a.kind}:${a.detail}`.localeCompare(`${b.kind}:${b.detail}`));
  entry.missing.sort();
}
inventory.entries.sort((a, b) => a.id.localeCompare(b.id));
inventory.texts.sort((a, b) => a.id.localeCompare(b.id));
inventory.gaps.sort((a, b) => `${a.ecosystem}:${a.id ?? ''}:${a.reason}`.localeCompare(`${b.ecosystem}:${b.id ?? ''}:${b.reason}`));
inventory.provenance.inputs.sort((a, b) => a.label.localeCompare(b.label));
const output = args.get('output');
fs.mkdirSync(path.dirname(path.resolve(output)), { recursive: true });
fs.writeFileSync(output, serializeInventory(inventory));
console.log(`Swift license inventory: ${inventory.entries.length} entries; ${inventory.gaps.length} explicit gaps.`);
