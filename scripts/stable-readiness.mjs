import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

export const recordPath = 'release/stable-readiness.json';
export const platforms = ['darwin-x86_64', 'darwin-aarch64', 'windows-x86_64', 'linux-x86_64'];
export const journeys = [
  'first-install-import-connect', 'profile-and-node-switch', 'dns-live-transitions',
  'capture-mode-transitions', 'restart-restore', 'kernel-upgrade-success-and-failure',
  'client-upgrade-and-interruption', 'normal-exit-cleanup', 'forced-exit-recovery',
  'invalid-config-and-network-recovery', 'diagnostics-and-state-consistency',
];
export const cases = [
  ...['disabled', 'real', 'fake-ip'].flatMap(dns =>
    ['off', 'system-proxy', 'tun', 'system-proxy-and-tun'].flatMap(capture =>
      ['ipv4', 'dual-stack'].map(network => `network/${dns}/${capture}/${network}`))),
  ...journeys.map(journey => `journey/${journey}`),
];
const git = (...args) => execFileSync('git', args, { maxBuffer: 32 * 1024 * 1024 });

// Bind evidence to the entire source tree, excluding only the evidence record.
// HEAD^ is the qualified source before the version-only release commit.
export function sourceFingerprint(ref = 'HEAD') {
  const entries = git('ls-tree', '-rz', '--full-tree', ref).toString().split('\0')
    .filter(entry => entry && entry.split('\t')[1] !== recordPath).sort();
  return createHash('sha256').update(entries.join('\0')).digest('hex');
}

export function readRecord(ref = 'HEAD') {
  return JSON.parse(git('show', `${ref}:${recordPath}`).toString());
}

export function assertReleaseDelta() {
  const allowed = new Set(['package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock',
    'src-tauri/tauri.conf.json', 'src-tauri/Info.plist']);
  const changed = git('diff', '--name-only', '-z', 'HEAD^', 'HEAD').toString().split('\0').filter(Boolean);
  if (!changed.length || changed.some(path => !allowed.has(path))) {
    throw new Error('Stable release commit must contain only version manifests; requalify source changes.');
  }
  for (const path of changed) {
    const before = git('show', `HEAD^:${path}`).toString();
    const after = git('show', `HEAD:${path}`).toString();
    if (normalizeManifest(path, before) !== normalizeManifest(path, after)) {
      throw new Error(`${path} contains changes beyond release version metadata`);
    }
  }
  // The record itself must be inherited unchanged from the qualified source.
}

export function normalizeManifest(path, content) {
  if (path.endsWith('.json')) {
    const value = JSON.parse(content);
    delete value.version;
    if (path.endsWith('tauri.conf.json')) {
      delete value.bundle?.windows?.wix?.version;
      delete value.bundle?.macOS?.bundleVersion;
    }
    return JSON.stringify(value);
  }
  if (path.endsWith('Cargo.toml')) {
    return content.replace(/(\[package\][\s\S]*?\nversion\s*=\s*)"[^"]+"/, '$1"VERSION"');
  }
  if (path.endsWith('Cargo.lock')) {
    return content.replace(/(\[\[package\]\]\nname = "gui"\nversion = )"[^"]+"/, '$1"VERSION"');
  }
  return content.replace(/(<key>CFBundle(?:ShortVersionString|Version)<\/key>\s*<string>)[^<]+/g, '$1VERSION');
}

const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value) && !/^([a-f0-9])\1+$/.test(value);
const evidenceUrl = value => {
  try { return new URL(value).protocol === 'https:'; } catch { return false; }
};

export function validateRecord(record, fingerprint) {
  const errors = [];
  if (record?.schemaVersion !== 1) errors.push('schemaVersion must be 1');
  if (record?.sourceFingerprint !== fingerprint) errors.push('source changed or has no qualified fingerprint');
  if (!/^[a-f0-9]{40}$/.test(record?.kernelCommit ?? '')) errors.push('paired kernel commit is missing');
  if (!Array.isArray(record?.blockers) || record.blockers.length) errors.push('release blockers remain');
  for (const platform of platforms) {
    const target = record?.platforms?.[platform];
    if (!digest(target?.clientArtifactSha256) || !digest(target?.kernelArtifactSha256)) {
      errors.push(`${platform}: paired tested artifact digests are missing`);
    }
    for (const id of cases) {
      const result = target?.cases?.[id];
      if (result?.status !== 'passed' || result?.level !== 'installed-e2e'
          || !evidenceUrl(result?.evidence) || !digest(result?.evidenceSha256)
          || typeof result?.tester !== 'string' || !result.tester.trim()
          || !Number.isFinite(Date.parse(result?.testedAt))) {
        errors.push(`${platform}: ${id} lacks installed-e2e evidence`);
      }
    }
  }
  return errors;
}
