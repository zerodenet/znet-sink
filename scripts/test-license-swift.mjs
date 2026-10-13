import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { simulatorTriplesFromBuildLog } from './license-swift-targets.mjs';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'znet-swift-license-fixture-'));
try {
  const checkout = path.join(root, 'checkouts/swift-rs');
  const tauri = path.join(root, 'cargo-tauri');
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
  const run = (extra = []) => {
    const output = path.join(root, 'inventory.json');
    execFileSync(process.execPath, [path.join(scripts, 'license-swift.mjs'), '--cargo-metadata', metadata,
      '--swift-build-path', root, '--output', output, ...extra], { stdio: 'pipe' });
    return JSON.parse(fs.readFileSync(output));
  };
  const inventory = run();
  const remote = inventory.entries.find((entry) => entry.name === 'SwiftRs');
  assert.equal(remote.version, '1.2.3');
  assert.equal(remote.declaredLicense, 'MIT');
  assert.equal(remote.textStatus, 'available');
  assert.ok(remote.evidence.some((entry) => entry.kind === 'resolved-target'));
  assert.ok(inventory.provenance.inputs.some((entry) => entry.label.endsWith('src-swift/Bridge.swift')));
  assert.equal(JSON.stringify(inventory).includes(root), false);
  assert.equal(inventory.entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'bundled' || evidence.kind === 'compiled')), false);
  assert.deepEqual(run(), inventory);
  const buildLog = path.join(root, 'build.log');
  fs.writeFileSync(buildLog, 'SwiftCompile normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'compiled')), false);
  fs.writeFileSync(buildLog, 'SDKROOT = iphoneos\nSwiftCompile normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.some((entry) => entry.evidence.some((evidence) => evidence.kind === 'compiled')), false);
  fs.writeFileSync(buildLog, 'SDKROOT = iphonesimulator\nSwiftDriver Compilation Tauri normal arm64 -module-name Tauri\n** BUILD SUCCEEDED **\n');
  assert.equal(run(['--build-log', buildLog]).entries.find((entry) => entry.name === 'Tauri').evidence.some((evidence) => evidence.kind === 'compiled'), true);
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
