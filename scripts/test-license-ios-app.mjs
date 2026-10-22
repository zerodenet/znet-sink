import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { mergeIosInventories } from './license-ios-app.mjs';
import { iosLicenseReceipt } from './license-ios-artifact.mjs';
import { sha256, serializeInventory } from './license-common.mjs';

const input = (platform, name) => ({schemaVersion: 1,
  target: {platform, triple: platform === 'frontend' ? 'webview' : 'aarch64-apple-ios-sim', profile: 'debug', features: ['tool-dns']},
  provenance: {kind: platform === 'ios' ? 'ios-application-swift-resolved-inputs' : 'rendered-frontend', inputs: []},
  entries: [{id: name, name, ecosystem: name === 'cargo' ? 'cargo' : name === 'frontend' ? 'npm' : 'swift',
    evidence: [{kind: 'resolved-target', detail: 'Synthetic test only'}], licenseTextIds: [], noticeTextIds: [], missing: ['original-material-missing']}],
  texts: [], gaps: [{ecosystem: 'test', reason: `${name}-original-material-missing`}]});
const cargo = input('ios', 'cargo'), frontend = input('frontend', 'frontend'), swift = input('ios', 'swift');
test('iOS assembly retains each actual input and material gap without inventing compiled evidence', () => {
  const merged = mergeIosInventories(cargo, frontend, [swift]);
  assert.equal(merged.entries.length, 3);
  assert.equal(merged.gaps.length, 3);
  assert.ok(merged.entries.every(entry => entry.evidence.every(item => item.kind !== 'compiled')));
  assert.equal(merged.provenance.inputs.find(input => input.label === 'frontend-inventory').sha256, sha256(JSON.stringify(frontend)));
  for (const patch of [value => value.target.platform = 'android', value => value.target.profile = 'unsigned-native-library',
    value => value.target.features = [], value => value.target.triple = 'aarch64-linux-android']) {
    const other = structuredClone(cargo); patch(other);
    assert.throws(() => mergeIosInventories(other, frontend, [swift]));
  }
  assert.throws(() => mergeIosInventories(cargo, frontend, []));
  assert.throws(() => mergeIosInventories(cargo, frontend, [{...swift, provenance: {kind: 'swift-source-and-unsigned-library'}}]));
});
test('actual fixed App asset rejects absent bytes, stale frontend, altered terms and material-cache drift', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'znet-ios-license-test-'));
  try {
    const app = path.join(directory, 'Fixture.app'), asset = path.join(app, 'assets/license-inventory.native.json');
    const expected = path.join(directory, 'expected.json'), frontFile = path.join(directory, 'frontend.json');
    fs.mkdirSync(path.dirname(asset), {recursive: true});
    fs.writeFileSync(frontFile, JSON.stringify(frontend));
    const merged = mergeIosInventories(cargo, frontend, [swift]);
    const label = 'resources/licenses/swift-package-materials.json';
    const cache = path.join(directory, label);
    fs.mkdirSync(path.dirname(cache), {recursive: true}); fs.writeFileSync(cache, 'fixture-only');
    merged.provenance.inputs.push({label, sha256: sha256('fixture-only')});
    const text = serializeInventory(merged);
    fs.writeFileSync(expected, text);
    assert.throws(() => iosLicenseReceipt(app, expected, frontFile, directory));
    fs.writeFileSync(asset, text);
    const receipt = iosLicenseReceipt(app, expected, frontFile, directory);
    assert.equal(receipt.entries, 3); assert.equal(receipt.gaps, 3);
    assert.equal(receipt.licenseAsset.sha256, sha256(text));
    fs.writeFileSync(asset, text + ' ');
    assert.throws(() => iosLicenseReceipt(app, expected, frontFile, directory), /differs/);
    fs.writeFileSync(asset, text); fs.writeFileSync(frontFile, JSON.stringify({...frontend, gaps: []}));
    assert.throws(() => iosLicenseReceipt(app, expected, frontFile, directory), /stale frontend/);
    fs.writeFileSync(frontFile, JSON.stringify(frontend)); fs.writeFileSync(cache, 'changed');
    assert.throws(() => iosLicenseReceipt(app, expected, frontFile, directory), /material input/);
    fs.writeFileSync(asset, Buffer.alloc(16 * 1024 * 1024 + 1));
    assert.throws(() => iosLicenseReceipt(app, expected, frontFile, directory), /budget/);
  } finally { fs.rmSync(directory, {recursive: true, force: true}); }
});
