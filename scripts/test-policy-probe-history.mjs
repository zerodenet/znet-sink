import assert from 'node:assert/strict';

import {
  buildPolicySnapshotHistoryUpdates,
  policyProbeEventFromSnapshot,
  policyProbeWaitTimeoutMs,
} from '../src/lib/services/policy-probe-history.ts';

function group(name, selected, outbounds, kind = 'url_test') {
  return { name, kind, selected, outbounds };
}

function testAdaptivePolicyProbeTimeout() {
  assert.equal(policyProbeWaitTimeoutMs(1), 60_000);
  assert.equal(policyProbeWaitTimeoutMs(5), 65_000);
  assert.equal(policyProbeWaitTimeoutMs(12), 135_000);
  assert.equal(policyProbeWaitTimeoutMs(1000), 600_000);
}

function testScheduledSnapshotAppendsMembersAndSelectedGroup() {
  const checkedAt = 1_785_514_472_491;
  const groups = [group('Auto - UrlTest', 'HK', [
    { tag: 'HK', type: 'shadowsocks', delayMs: 91, alive: true, lastCheckedUnixMs: checkedAt },
    { tag: 'JP', type: 'trojan', alive: false, lastCheckedUnixMs: checkedAt, lastError: 'timeout' },
  ])];

  const updates = buildPolicySnapshotHistoryUpdates(groups);
  assert.deepEqual(updates, [
    { tag: 'HK', delayMs: 91, reachable: true, at: checkedAt },
    { tag: 'Auto - UrlTest', delayMs: 91, reachable: true, at: checkedAt, selectedTag: 'HK' },
    { tag: 'JP', delayMs: undefined, reachable: false, at: checkedAt },
  ]);
}

function testSnapshotRecoversOnlyFreshPendingProbe() {
  const requestedAt = 10_000;
  const stale = group('Auto', 'HK', [
    { tag: 'HK', type: 'shadowsocks', delayMs: 80, alive: true, lastCheckedUnixMs: 9_999 },
  ]);
  assert.equal(policyProbeEventFromSnapshot(stale, requestedAt, 'manual'), undefined);

  const fresh = group('Auto', 'HK', [
    { tag: 'HK', type: 'shadowsocks', delayMs: 75, alive: true, lastCheckedUnixMs: 10_001 },
  ]);
  assert.deepEqual(policyProbeEventFromSnapshot(fresh, requestedAt, 'manual'), {
    policyTag: 'Auto',
    trigger: 'manual',
    completedAtUnixMs: 10_001,
    selected: 'HK',
    members: fresh.outbounds,
  });
}

testAdaptivePolicyProbeTimeout();
testScheduledSnapshotAppendsMembersAndSelectedGroup();
testSnapshotRecoversOnlyFreshPendingProbe();

console.log('policy probe history tests passed');
