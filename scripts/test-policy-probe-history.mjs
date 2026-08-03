import assert from 'node:assert/strict';

import {
  buildPolicyProbeTimeoutUpdate,
  buildPolicySnapshotHistoryUpdates,
  isPolicyProbeEventFresh,
  policyProbeEventFromSnapshot,
  policyProbeWaitTimeoutMs,
} from '../src/lib/services/policy-probe-history.ts';
import {
  buildAllNodes,
  buildRuntimeOverlay,
  resolveProbeDisplay,
} from '../src/lib/components/tabs/nodes-view-model.ts';

function group(name, selected, outbounds, kind = 'url_test') {
  return { name, kind, selected, outbounds };
}

function testAdaptivePolicyProbeTimeout() {
  assert.equal(policyProbeWaitTimeoutMs(1), 60_000);
  assert.equal(policyProbeWaitTimeoutMs(5), 65_000);
  assert.equal(policyProbeWaitTimeoutMs(12), 135_000);
  assert.equal(policyProbeWaitTimeoutMs(1000), 600_000);
}

function testFreshScheduledResultCanSettleManualWaiter() {
  const requestedAt = 10_000;
  assert.equal(isPolicyProbeEventFresh({
    policyTag: 'Auto',
    trigger: 'scheduled',
    completedAtUnixMs: 10_001,
    members: [],
  }, requestedAt), true);
  assert.equal(isPolicyProbeEventFresh({
    policyTag: 'Auto',
    trigger: 'manual',
    completedAtUnixMs: 9_999,
    members: [],
  }, requestedAt), false);
  // Older kernels can omit timing metadata; keep their events compatible.
  assert.equal(isPolicyProbeEventFresh({
    policyTag: 'Auto',
    trigger: 'scheduled',
    members: [],
  }, requestedAt), true);
}

function testPolicyTimeoutProducesVisibleGroupObservation() {
  const update = buildPolicyProbeTimeoutUpdate([
    group('Auto', 'HK', [{ tag: 'HK' }]),
  ], 'Auto', 12_345);
  assert.deepEqual(update, {
    tag: 'Auto',
    reachable: false,
    at: 12_345,
    selectedTag: 'HK',
  });
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

function testSameTimestampHistoryOverridesStaleRuntimeValue() {
  assert.deepEqual(resolveProbeDisplay({
    runtimeDelay: 999,
    runtimeAt: 20_000,
    localDelay: 35,
    localAt: 20_000,
  }), {
    delay: 35,
    at: 20_000,
  });
}

function testNestedUrlTestCardUsesOwnPolicyHistory() {
  const checkedAt = 30_000;
  const groups = [
    group('Proxy', 'Auto', [
      { tag: 'Auto', type: 'url_test', delayMs: 999, alive: true, lastCheckedUnixMs: checkedAt },
    ], 'selector'),
    group('Auto', 'HK', [
      { tag: 'HK', type: 'shadowsocks', delayMs: 35, alive: true, lastCheckedUnixMs: checkedAt },
    ], 'url_test'),
  ];

  const [auto] = buildAllNodes({
    configNodes: [{ tag: 'Auto', protocol: 'url_test', isSelector: true }],
    groups,
    runtimeOverlay: buildRuntimeOverlay(groups),
    latestDelay: (tag) => tag === 'Auto' ? 35 : undefined,
    latestProbeTime: (tag) => tag === 'Auto' ? checkedAt : undefined,
    fallbackNodes: [],
  });

  assert.equal(auto.delay, 35);
  assert.equal(auto.lastProbeAt, checkedAt);
  assert.equal(auto.protocol, 'url_test');
}

testAdaptivePolicyProbeTimeout();
testFreshScheduledResultCanSettleManualWaiter();
testPolicyTimeoutProducesVisibleGroupObservation();
testScheduledSnapshotAppendsMembersAndSelectedGroup();
testSnapshotRecoversOnlyFreshPendingProbe();
testSameTimestampHistoryOverridesStaleRuntimeValue();
testNestedUrlTestCardUsesOwnPolicyHistory();

console.log('policy probe history tests passed');
