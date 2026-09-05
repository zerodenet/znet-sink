import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { cases, platforms, validateRecord, normalizeManifest } from './stable-readiness.mjs';

const fingerprint = 'a1'.repeat(32);
function qualified() {
  return {
    schemaVersion: 1, sourceFingerprint: fingerprint, kernelCommit: 'a1'.repeat(20), blockers: [],
    platforms: Object.fromEntries(platforms.map(platform => [platform, {
      clientArtifactSha256: 'b2'.repeat(32), kernelArtifactSha256: 'c3'.repeat(32),
      cases: Object.fromEntries(cases.map(id => [id, {
        status: 'passed', level: 'installed-e2e', evidence: 'https://ci.example/run/123',
        evidenceSha256: 'd4'.repeat(32), testedAt: '2026-09-05T10:00:00Z', tester: 'test-runner',
      }])),
    }])),
  };
}
test('complete paired evidence is accepted', () => {
  assert.equal(cases.length, 35);
  assert.deepEqual(validateRecord(qualified(), fingerprint), []);
});
test('release metadata exception cannot hide dependency changes', () => {
  const cargo = '[package]\nname = "gui"\nversion = "1.0.0-rc.1"\n[dependencies]\nx = "1"';
  assert.equal(normalizeManifest('Cargo.toml', cargo), normalizeManifest('Cargo.toml', cargo.replace('1.0.0-rc.1', '1.0.0')));
  assert.notEqual(normalizeManifest('Cargo.toml', cargo), normalizeManifest('Cargo.toml', cargo.replace('x = "1"', 'x = "2"')));
  const lock = '[[package]]\nname = "gui"\nversion = "1.0.0"\n[[package]]\nname = "dep"\nversion = "1.0.0"';
  assert.notEqual(normalizeManifest('Cargo.lock', lock), normalizeManifest('Cargo.lock', lock.replace('name = "dep"\nversion = "1.0.0"', 'name = "dep"\nversion = "2.0.0"')));
});
test('every platform and network/journey case is mandatory', () => {
  for (const platform of platforms) {
    for (const id of cases) {
      const record = qualified();
      delete record.platforms[platform].cases[id];
      assert.ok(validateRecord(record, fingerprint).length, `${platform}/${id}`);
    }
  }
});
test('unit success, missing artifacts, stale source and blockers cannot qualify stable', () => {
  for (const mutate of [
    r => { r.platforms[platforms[0]].cases[cases[0]].level = 'unit'; },
    r => { r.platforms[platforms[0]].cases[cases[0]].status = 'pending'; },
    r => { r.platforms[platforms[0]].cases[cases[0]].evidence = ''; },
    r => { r.platforms[platforms[0]].clientArtifactSha256 = '0'.repeat(64); },
    r => { r.sourceFingerprint = 'stale'; },
    r => { r.kernelCommit = ''; },
    r => { r.blockers = ['unrecovered interrupted upgrade']; },
  ]) {
    const record = qualified(); mutate(record);
    assert.ok(validateRecord(record, fingerprint).length);
  }
  assert.ok(validateRecord(null, fingerprint).length);
});

test('CLI binds committed evidence to source and permits only a version release commit', () => {
  const root = mkdtempSync(join(tmpdir(), 'stable-gate-'));
  const git = (...args) => execFileSync('git', args, { cwd: root, stdio: 'pipe' });
  const run = (...args) => spawnSync(process.execPath, ['scripts/check-stable-readiness.mjs', ...args], { cwd: root, encoding: 'utf8' });
  const commit = () => { git('add', '.'); git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-m', 'test'); };
  try {
    git('init');
    mkdirSync(join(root, 'scripts'));
    mkdirSync(join(root, 'release'));
    for (const file of ['check-stable-readiness.mjs', 'stable-readiness.mjs']) {
      copyFileSync(new URL(file, import.meta.url), join(root, 'scripts', file));
    }
    const manifest = { version: '1.0.0-rc.1', dependencies: { x: '1' } };
    const writeManifest = () => writeFileSync(join(root, 'package.json'), JSON.stringify(manifest));
    writeManifest(); commit();
    const record = qualified();
    record.sourceFingerprint = run('--fingerprint').stdout.trim();
    writeFileSync(join(root, 'release/stable-readiness.json'), JSON.stringify(record));
    commit();
    assert.equal(run().status, 0, run().stderr);
    manifest.version = '1.0.0'; writeManifest();
    assert.equal(run().status, 1, 'dirty work must be rejected');
    commit();
    assert.equal(run('--release').status, 0, run('--release').stderr);
    assert.equal(run().status, 1, 'new source needs requalification for a future release');
    manifest.dependencies.x = '2'; writeManifest(); commit();
    assert.equal(run('--release').status, 1, 'dependency changes cannot masquerade as version metadata');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
