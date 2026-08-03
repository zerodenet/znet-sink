import assert from 'node:assert/strict';
import {
  retainConfiguredPolicyGroups,
  shouldApplyPolicyProbeEvent,
} from '../src/lib/services/node-state-reconcile.ts';

const group = (name) => ({ name, kind: 'selector', outbounds: [] });
const oldRuntime = [group('old-only'), group('shared')];
const nextConfig = [group('shared'), group('new-only')];

assert.deepEqual(
  retainConfiguredPolicyGroups(oldRuntime, nextConfig).map((item) => item.name),
  ['shared'],
  'config switch should remove runtime groups owned only by the old profile',
);
assert.equal(
  shouldApplyPolicyProbeEvent(nextConfig, oldRuntime, 'old-only'),
  false,
  'a late old-profile probe event must not resurrect its policy group',
);
assert.equal(
  shouldApplyPolicyProbeEvent(nextConfig, oldRuntime, 'new-only'),
  true,
  'the active config may accept an event before its runtime snapshot arrives',
);
assert.equal(
  shouldApplyPolicyProbeEvent([], [group('runtime-only')], 'runtime-only'),
  true,
  'runtime-only groups may continue receiving probe events',
);
assert.equal(
  shouldApplyPolicyProbeEvent([], [], 'unknown'),
  false,
  'an event alone must not create an unknown group',
);

console.log('node state reconcile tests passed');
