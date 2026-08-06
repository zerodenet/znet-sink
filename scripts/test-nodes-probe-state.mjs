import assert from 'node:assert/strict';
import {
  applyProbeJobSnapshot,
  mergeActiveProbeJobs,
  shouldApplyNodeScreenSnapshot,
} from '$lib/components/tabs/nodes-probe-state';

const job = (id, state, updatedAtUnixMs, extra = {}) => ({
  id,
  scope: { profileId: 'profile-a', configRevision: 1, coreInstanceId: 1 },
  kind: 'outbound',
  state,
  targetTags: ['node-a'],
  results: [],
  completed: 0,
  succeeded: 0,
  failed: 0,
  startedAtUnixMs: 100,
  updatedAtUnixMs,
  deadlineAtUnixMs: 30_100,
  ...extra,
});

{
  let state = { directJobs: new Map(), terminalJobIds: new Set() };
  state = applyProbeJobSnapshot(state, job(1, 'running', 101));
  assert.equal(mergeActiveProbeJobs([], state.directJobs, state.terminalJobIds).length, 1);

  state = applyProbeJobSnapshot(state, job(1, 'failed', 200, { completed: 1, failed: 1 }));
  assert.equal(mergeActiveProbeJobs([job(1, 'running', 150)], state.directJobs, state.terminalJobIds).length, 0);

  // A very fast terminal event can arrive before the start invoke resolves.
  // The delayed running response must not resurrect the spinner.
  state = applyProbeJobSnapshot(state, job(1, 'running', 101));
  assert.equal(state.directJobs.has(1), false);
  assert.equal(state.terminalJobIds.has(1), true);
}

{
  let state = { directJobs: new Map(), terminalJobIds: new Set() };
  state = applyProbeJobSnapshot(state, job(2, 'running', 200));
  const ignored = applyProbeJobSnapshot(state, job(2, 'running', 150));
  assert.equal(ignored.directJobs.get(2).updatedAtUnixMs, 200);
}

assert.equal(shouldApplyNodeScreenSnapshot({
  currentRevision: 8,
  candidateRevision: 9,
  requestSequence: 4,
  lastAppliedRequest: 3,
}), true);
assert.equal(shouldApplyNodeScreenSnapshot({
  currentRevision: 9,
  candidateRevision: 8,
  requestSequence: 5,
  lastAppliedRequest: 3,
}), false);
assert.equal(shouldApplyNodeScreenSnapshot({
  currentRevision: 8,
  candidateRevision: 9,
  requestSequence: 2,
  lastAppliedRequest: 3,
}), false);

console.log('nodes probe state tests passed');
