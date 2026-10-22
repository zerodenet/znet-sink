import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { simulatorTriplesFromBuildLog } from './license-swift-targets.mjs';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'znet-swift-license-fixture-'));
try {
  const checkout = path.join(root, 'checkouts/swift-rs');
  const tauri = path.join(root, 'tauri-9.9.9');
  const api = path.join(tauri, 'mobile/ios-api');
  fs.mkdirSync(path.join(checkout, 'src-swift'), { recursive: true });
  fs.mkdirSync(path.join(api, 'Sources/Tauri'), { recursive: true });
  fs.writeFileSync(path.join(checkout, 'Package.swift'), '// swift-tools-version:5.7\nimport PackageDescription\nlet package = Package(name: "SwiftRs", targets: [.target(name: "SwiftRs", path: "src-swift")])\n');
  fs.writeFileSync(path.join(checkout, 'LICENSE'), 'SYNTHETIC TEST LICENSE ONLY\n');
  fs.writeFileSync(path.join(checkout, 'src-swift/Bridge.swift'), '// Copyright (c) Synthetic fixture\n// SPDX-License-Identifier: MIT\nimport Foundation\n');
  fs.writeFileSync(path.join(tauri, 'Cargo.toml'), '[package]\nname="tauri"\nversion="9.9.9"\n');
  fs.writeFileSync(path.join(tauri, 'LICENSE'), 'SYNTHETIC TAURI TEST LICENSE ONLY\n');
  fs.writeFileSync(path.join(api, 'Package.swift'), '// swift-tools-version:5.7\nimport PackageDescription\nlet package = Package(name: "Tauri", dependencies: [.package(name: "SwiftRs", url: "https://github.com/Brendonovich/swift-rs", from: "1.0.0")], targets: [.target(name: "Tauri", dependencies: ["SwiftRs"], path: "Sources")])\n');
  fs.writeFileSync(path.join(api, 'Sources/Tauri/Bridge.swift'), '// Copyright (c) Synthetic Tauri fixture\n// SPDX-License-Identifier: MIT\nimport Foundation\nimport SwiftRs\n');
  const archive = path.join(root, 'tauri-9.9.9.crate');
  execFileSync('tar', ['-cf', archive, '-C', root, 'tauri-9.9.9']);
  const checksum = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
  const cargoLock = path.join(root, 'Cargo.lock');
  fs.writeFileSync(cargoLock, `[[package]]\nname = "tauri"\nversion = "9.9.9"\nsource = "registry+https://fixtures.invalid"\nchecksum = "${checksum}"\n`);
  const metadata = path.join(root, 'cargo-metadata.json');
  fs.writeFileSync(metadata, JSON.stringify({ packages: [
    { id: 'native-io-fixture', name: 'tauri-plugin-znet-native-io', version: '0.0.3', license: 'MPL-2.0' },
    { id: 'tauri-fixture', name: 'tauri', version: '9.9.9', license: 'MIT', manifest_path: path.join(tauri, 'Cargo.toml'), source: 'registry+https://fixtures.invalid' }
  ], resolve: { nodes: [{ id: 'native-io-fixture', deps: [{ name: 'tauri', pkg: 'tauri-fixture' }] }] } }));
  execFileSync('git', ['init', '-q', checkout]);
  execFileSync('git', ['-C', checkout, 'add', '.']);
  execFileSync('git', ['-C', checkout, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']);
  const revision = execFileSync('git', ['-C', checkout, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const lock = { version: 2, pins: [{ identity: 'swift-rs', location: 'https://github.com/Brendonovich/swift-rs', state: { revision, version: '1.2.3' } }] };
  fs.writeFileSync(path.join(root, 'Package.resolved'), JSON.stringify(lock));
  fs.writeFileSync(path.join(root, 'workspace-state.json'), JSON.stringify({ object: { dependencies: [{ packageRef: { location: 'https://github.com/Brendonovich/swift-rs' }, subpath: 'swift-rs', state: { checkoutState: { revision, version: '1.2.3' } } }] } }));
  const run = (extra = [], nodeOptions = []) => {
    const output = path.join(root, 'inventory.json');
    execFileSync(process.execPath, [...nodeOptions, path.join(scripts, 'license-swift.mjs'), '--cargo-metadata', metadata,
      '--swift-build-path', root, '--cargo-lock', cargoLock, '--cargo-archive', archive, '--output', output, ...extra], { stdio: 'pipe' });
    return JSON.parse(fs.readFileSync(output));
  };
  const inventory = run();
  assert.ok(inventory.entries.some((entry) => entry.name === 'tauri-plugin-znet-native-io'));
  assert.ok(!inventory.entries.some((entry) => entry.name === 'znet-native-io'));
  const remote = inventory.entries.find((entry) => entry.name === 'SwiftRs');
  assert.equal(remote.version, '1.2.3');
  assert.equal(remote.declaredLicense, 'MIT');
  assert.equal(remote.textStatus, 'available');
  assert.ok(remote.evidence.some((entry) => entry.kind === 'resolved-target'));
  assert.ok(inventory.provenance.inputs.some((entry) => entry.label.endsWith('src-swift/Bridge.swift')));
  assert.equal(JSON.stringify(inventory).includes(root), false);
  assert.equal(inventory.entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'bundled' || evidence.kind === 'compiled')), false);
  assert.deepEqual(run(), inventory);
  const cacheFile = path.join(root, 'package-materials.json');
  const cacheText = 'SYNTHETIC REVIEWED PACKAGE LICENSE ONLY\nCopyright Synthetic exact package\n';
  const cacheHash = createHash('sha256').update(cacheText).digest('hex');
  const packageCache = { schemaVersion: 1, packages: [{
    identity: { ecosystem: 'swift', name: 'SwiftRs', version: '1.2.3', source: `https://github.com/Brendonovich/swift-rs#${revision}` },
    expected: { manifestSha256: createHash('sha256').update(fs.readFileSync(path.join(api, 'Package.swift'))).digest('hex'), checkoutRevision: revision },
    materials: [{ name: 'LICENSE', kind: 'license', source: `https://fixtures.invalid/${revision}/LICENSE`, text: cacheText, sha256: cacheHash,
      origin: { url: `https://fixtures.invalid/${revision}/LICENSE`, sha256: cacheHash, revision } }], resolves: []
  }] };
  fs.writeFileSync(cacheFile, JSON.stringify(packageCache));
  // A real lock pin can bind offline legal material without claiming a checkout
  // or compile. Absence of the actual pin cannot be repaired by this cache.
  fs.renameSync(checkout, `${checkout}-away`);
  const cached = run(['--materials-cache', cacheFile]);
  const cachedRemote = cached.entries.find((entry) => entry.name === 'SwiftRs');
  assert.equal(cachedRemote.textStatus, 'available');
  assert.ok(cachedRemote.missing.some((issue) => issue.includes('checkout')));
  assert.ok(cached.provenance.inputs.some((entry) => entry.label === 'resources/licenses/swift-package-materials.json'));
  assert.ok(cached.texts.some((entry) => entry.sha256 === cacheHash));
  fs.renameSync(path.join(root, 'Package.resolved'), path.join(root, 'resolved-away.json'));
  const noPin = run(['--materials-cache', cacheFile]).entries.find((entry) => entry.name === 'SwiftRs');
  assert.equal(noPin.version, 'unresolved');
  assert.equal(noPin.textStatus, 'missing');
  assert.equal(noPin.licenseTextIds.length, 0);
  fs.renameSync(path.join(root, 'resolved-away.json'), path.join(root, 'Package.resolved'));
  packageCache.packages[0].expected.manifestSha256 = '0'.repeat(64);
  fs.writeFileSync(cacheFile, JSON.stringify(packageCache));
  assert.throws(() => run(['--materials-cache', cacheFile]), /Command failed/);
  fs.renameSync(`${checkout}-away`, checkout);
  const originalLicense = fs.readFileSync(path.join(checkout, 'LICENSE'));
  fs.writeFileSync(path.join(checkout, 'LICENSE'), 'FORGED uncommitted legal body');
  assert.throws(() => run(), /Command failed/);
  fs.writeFileSync(path.join(checkout, 'LICENSE'), originalLicense);
  const originalTauriLicense = fs.readFileSync(path.join(tauri, 'LICENSE'));
  fs.writeFileSync(path.join(tauri, 'LICENSE'), 'FORGED Cargo extracted legal body');
  assert.throws(() => run(), /Command failed/);
  fs.writeFileSync(path.join(tauri, 'LICENSE'), originalTauriLicense);
  const statePath = path.join(root, 'workspace-state.json');
  const workspace = fs.readFileSync(statePath, 'utf8');
  const staleWorkspace = JSON.parse(workspace);
  staleWorkspace.object.dependencies[0].state.checkoutState.revision = 'f'.repeat(40);
  fs.writeFileSync(statePath, JSON.stringify(staleWorkspace));
  assert.equal(run().entries.find((entry) => entry.name === 'SwiftRs').textStatus, 'missing');
  fs.writeFileSync(statePath, workspace);
  const locked = fs.readFileSync(cargoLock, 'utf8');
  fs.writeFileSync(cargoLock, locked.replace(checksum, '0'.repeat(64)));
  assert.throws(() => run(), /Command failed/);
  fs.writeFileSync(cargoLock, locked);
  const sdkFile = path.join(root, 'SDKSettings.json');
  fs.writeFileSync(sdkFile, JSON.stringify({ CanonicalName: 'iphoneos18.5', Version: '18.5' }));
  assert.throws(() => run(['--sdk-data', sdkFile]), /Command failed/);
  fs.writeFileSync(sdkFile, JSON.stringify({ CanonicalName: 'iphonesimulator18.5', Version: '18.5' }));
  const sdkInventory = run(['--sdk-data', sdkFile]);
  assert.ok(sdkInventory.entries.filter((entry) => entry.ecosystem === 'platform').every((entry) => entry.textStatus === 'not-applicable' && entry.licenseTextIds.length === 0));
  assert.ok(sdkInventory.gaps.some((entry) => entry.reason.includes('without an SDK root')));
  // Exercise actual Darwin discovery branches on Linux too, without depending
  // on the runner's Xcode installation or disabling real discovery in production.
  const fakeSdk = path.join(root, 'fixture-simulator-sdk');
  fs.mkdirSync(fakeSdk);
  fs.copyFileSync(sdkFile, path.join(fakeSdk, 'SDKSettings.json'));
  const darwinPreload = path.join(root, 'darwin-sdk-fixture.mjs');
  fs.writeFileSync(darwinPreload, `import child from 'node:child_process';\nimport {syncBuiltinESMExports} from 'node:module';\nObject.defineProperty(process,'platform',{value:'darwin'});\nconst original=child.execFileSync;\nchild.execFileSync=(file,...args)=>file==='xcrun'?${JSON.stringify(fakeSdk)}:original(file,...args);\nsyncBuiltinESMExports();\n`);
  const explicitMetadata = run(['--sdk-data', sdkFile], ['--import', darwinPreload]);
  assert.ok(explicitMetadata.gaps.some((entry) => entry.reason.includes('without an SDK root')));
  assert.ok(!explicitMetadata.provenance.inputs.some((entry) => entry.label.includes('.framework/')));
  const discoveredSdk = run([], ['--import', darwinPreload]);
  assert.ok(!discoveredSdk.gaps.some((entry) => entry.reason.includes('without an SDK root') || entry.reason.includes('Apple SDK metadata unavailable')));
  assert.ok(discoveredSdk.provenance.inputs.some((entry) => entry.label === 'apple-sdk/SDKSettings.json'));
  assert.ok(!run(['--sdk-data', sdkFile, '--sdk-path', fakeSdk]).gaps.some((entry) => entry.reason.includes('without an SDK root')));
  fs.writeFileSync(path.join(fakeSdk, 'SDKSettings.json'), JSON.stringify({CanonicalName:'iphonesimulator99.0',Version:'99.0'}));
  assert.throws(() => run(['--sdk-data', sdkFile, '--sdk-path', fakeSdk]), /Command failed/);
  const buildLog = path.join(root, 'build.log');
  fs.writeFileSync(buildLog, 'SwiftCompile normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'compiled')), false);
  fs.writeFileSync(buildLog, 'SDKROOT = iphoneos\nSwiftCompile normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'compiled')), false);
  fs.writeFileSync(buildLog, 'SDKROOT = iphonesimulator\nSwiftDriver Compilation Tauri normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.find((entry) => entry.name === 'Tauri').evidence.some((evidence) => evidence.kind === 'compiled'), true);
  fs.writeFileSync(buildLog, 'SDKROOT = iphonesimulator\nSwiftDriver Compilation znet_native_io normal arm64 -module-name znet_native_io\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.find((entry) => entry.name === 'tauri-plugin-znet-native-io').evidence.some((evidence) => evidence.kind === 'compiled'), true);
  lock.pins[0].state.revision = '0'.repeat(40);
  fs.writeFileSync(path.join(root, 'Package.resolved'), JSON.stringify(lock));
  const stale = run().entries.find((entry) => entry.name === 'SwiftRs');
  assert.equal(stale.textStatus, 'missing');
  assert.equal(stale.licenseTextIds.length, 0);
  assert.ok(stale.missing.some((entry) => entry.includes('does not match')));
  fs.writeFileSync(path.join(root, 'fake.a'), '!<arch>\n');
  assert.throws(() => run(['--artifact', path.join(root, 'fake.a')]));
  assert.deepEqual(simulatorTriplesFromBuildLog('swiftc -target x86_64-apple-ios15.0-simulator\n** BUILD SUCCEEDED **'), ['x86_64-apple-ios']);
  assert.deepEqual(simulatorTriplesFromBuildLog('swiftc -target arm64-apple-ios15.0-simulator\nswiftc -target x86_64-apple-ios15.0-simulator\n** BUILD SUCCEEDED **'), ['aarch64-apple-ios-sim', 'x86_64-apple-ios']);
  assert.deepEqual(simulatorTriplesFromBuildLog('xcodebuild -sdk iphonesimulator\nSwiftDriver Compilation Tauri normal x86_64\n** BUILD SUCCEEDED **'), ['x86_64-apple-ios']);
  assert.throws(() => simulatorTriplesFromBuildLog('SwiftCompile normal arm64\n** BUILD SUCCEEDED **'));
  assert.throws(() => simulatorTriplesFromBuildLog('swiftc -target arm64-apple-ios15.0\n** BUILD SUCCEEDED **'));
  assert.throws(() => simulatorTriplesFromBuildLog('swiftc -target arm64-apple-ios15.0-simulator\n** BUILD FAILED **'));
  console.log('Swift inventory fixtures passed: pins, checkout identity, materials, target evidence, archive rejection, deterministic output and actual multi-architecture derivation. Synthetic fixtures do not prove a real Apple build.');
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
