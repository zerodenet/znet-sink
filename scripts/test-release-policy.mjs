import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  cleanupTagsForPublishedTag,
  formatReleaseTimestamp,
  parseRequestedVersion,
  resolveReleasePlan,
  selectPreviousReleaseTag,
  validatePublishedRelease,
} from './release-policy.mjs';

const now = new Date('2026-08-18T10:36:00Z');
assert.equal(formatReleaseTimestamp(now), '202608181036');
assert.deepEqual(parseRequestedVersion('0.0.17'), { baseVersion: '0.0.17', requestedChannel: 'stable' });
assert.deepEqual(parseRequestedVersion('0.0.17-rc'), { baseVersion: '0.0.17', requestedChannel: 'rc' });
for (const invalid of ['v0.0.17', '0.0.17-dev', '0.0.17-beta', '0.0.17-preview', '0.0.17-rc.1']) {
  assert.throws(() => parseRequestedVersion(invalid));
}

const stableTags = ['v0.0.15', 'v0.0.16'];
const dev = resolveReleasePlan({ branch: 'develop', input: '0.0.17', tags: stableTags, now, buildNumber: 101 });
assert.equal(dev.releaseVersion, '0.0.17-dev.202608181036');
assert.equal(dev.tag, 'v0.0.17-dev.202608181036');
assert.equal(dev.channel, 'dev');
assert.equal(dev.buildNumber, 101);

assert.throws(() => resolveReleasePlan({ branch: 'develop', input: '0.0.16', tags: stableTags, now, buildNumber: 101 }), /sealed/);
assert.throws(() => resolveReleasePlan({ branch: 'main', input: '0.0.17-rc', tags: stableTags, now, buildNumber: 101 }), /requires an existing dev/);

const withDev = [...stableTags, 'v0.0.17-dev.202608181036'];
const rc = resolveReleasePlan({ branch: 'main', input: '0.0.17-rc', tags: withDev, now, buildNumber: 102 });
assert.equal(rc.releaseVersion, '0.0.17-rc.202608181036');
assert.throws(() => resolveReleasePlan({ branch: 'develop', input: '0.0.17', tags: [...withDev, rc.tag], now: new Date('2026-08-18T10:37:00Z'), buildNumber: 103 }), /cannot return to dev/);

const withRc = [...withDev, rc.tag];
const stable = resolveReleasePlan({ branch: 'main', input: '0.0.17', tags: withRc, now, buildNumber: 103 });
assert.equal(stable.releaseVersion, '0.0.17');
assert.equal(stable.channel, 'stable');
assert.throws(() => resolveReleasePlan({ branch: 'main', input: '0.0.18', tags: withRc, now, buildNumber: 103 }), /active release line/);

const cleanedRemoteHistory = ['v0.0.16', 'v0.0.17-rc.202608181030'];
const recoveredRc = resolveReleasePlan({
  branch: 'main',
  input: '0.0.17-rc',
  tags: cleanedRemoteHistory,
  now: new Date('2026-08-18T10:38:00Z'),
  buildNumber: 104,
});
assert.equal(recoveredRc.releaseVersion, '0.0.17-rc.202608181038');
assert.equal(
  validatePublishedRelease({
    branch: 'main',
    tag: recoveredRc.tag,
    tags: [...cleanedRemoteHistory, recoveredRc.tag],
  }).channel,
  'rc',
);

assert.equal(
  validatePublishedRelease({ branch: 'develop', tag: dev.tag, tags: [...stableTags, dev.tag] }).channel,
  'dev',
);
assert.equal(
  validatePublishedRelease({ branch: 'main', tag: rc.tag, tags: withRc }).channel,
  'rc',
);
assert.equal(
  validatePublishedRelease({ branch: 'main', tag: 'v0.0.17', tags: [...withRc, 'v0.0.17'] }).channel,
  'stable',
);
assert.throws(
  () => validatePublishedRelease({ branch: 'main', tag: 'v0.0.18', tags: [...withRc, 'v0.0.18'] }),
  /active release line|requires an existing rc/,
);
assert.throws(
  () => validatePublishedRelease({ branch: 'main', tag: 'v0.0.17-beta.1', tags: withRc }),
  /not managed/,
);

const cleanupInput = [
  'v0.0.16',
  'v0.0.17-dev.202608181000',
  'v0.0.17-dev.202608181036',
  'v0.0.17-rc.202608181036',
  'v0.0.17-rc.202608181100',
  'v0.0.17',
];
assert.deepEqual(
  cleanupTagsForPublishedTag('v0.0.17-rc.202608181100', cleanupInput),
  ['v0.0.17-dev.202608181000', 'v0.0.17-dev.202608181036', 'v0.0.17-rc.202608181036'],
);
assert.deepEqual(
  cleanupTagsForPublishedTag('v0.0.17', cleanupInput),
  ['v0.0.17-dev.202608181000', 'v0.0.17-dev.202608181036', 'v0.0.17-rc.202608181036', 'v0.0.17-rc.202608181100'],
);
assert.deepEqual(cleanupTagsForPublishedTag('v0.0.17-dev.202608181036', cleanupInput), []);
assert.ok(!cleanupTagsForPublishedTag('v0.0.17', cleanupInput).includes('v0.0.16'));
assert.ok(!cleanupTagsForPublishedTag('v0.0.17', cleanupInput).includes('v0.0.17'));

const emptyCleanupCli = spawnSync(
  process.execPath,
  ['scripts/release-policy.mjs', 'cleanup', 'v0.0.17-rc.202608181100', 'v0.0.17-rc.202608181100'],
  { encoding: 'utf8' },
);
assert.equal(emptyCleanupCli.status, 0, emptyCleanupCli.stderr);
assert.equal(emptyCleanupCli.stdout, '');

const populatedCleanupCli = spawnSync(
  process.execPath,
  ['scripts/release-policy.mjs', 'cleanup', 'v0.0.17-rc.202608181100', ...cleanupInput],
  { encoding: 'utf8' },
);
assert.equal(populatedCleanupCli.status, 0, populatedCleanupCli.stderr);
assert.equal(
  populatedCleanupCli.stdout,
  'v0.0.17-dev.202608181000\nv0.0.17-dev.202608181036\nv0.0.17-rc.202608181036\n',
);

assert.equal(selectPreviousReleaseTag('v0.0.17-rc.202608181100', cleanupInput), 'v0.0.17-rc.202608181036');
assert.equal(selectPreviousReleaseTag('v0.0.17-rc.202608181036', ['v0.0.16', 'v0.0.17-dev.202608181000']), 'v0.0.16');
assert.equal(selectPreviousReleaseTag('v0.0.17', cleanupInput), 'v0.0.17-rc.202608181100');
assert.equal(selectPreviousReleaseTag('v0.0.17-rc.10', ['v0.0.16', 'v0.0.17-rc.8', 'v0.0.17-rc.10']), 'v0.0.17-rc.8');

console.log('release policy tests passed');
