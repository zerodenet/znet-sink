import assert from 'node:assert/strict';
import { buildConnectionView } from '../src/lib/services/connection-view.ts';

function connection(flowId, overrides = {}) {
  return {
    flowId,
    network: 'tcp',
    destination: 'example.com:443',
    bytesUp: 0,
    bytesDown: 0,
    selectionChain: [],
    relayChain: [],
    ...overrides,
  };
}

{
  const view = buildConnectionView({
    activeSnapshot: [connection('closed-during-query', {
      revision: 3,
      state: 'active',
      startedAtUnixMs: 1_000,
      updatedAtUnixMs: 2_000,
    })],
    recentSnapshot: [],
    activeEvents: [],
    recentEvents: [connection('closed-during-query', {
      revision: 4,
      state: 'completed',
      startedAtUnixMs: 1_000,
      endedAtUnixMs: 2_100,
      updatedAtUnixMs: 2_100,
      durationMs: 1_100,
    })],
  });

  assert.equal(view.active.length, 0, 'a completion event must suppress a stale active snapshot');
  assert.equal(view.recent.length, 1, 'the completed connection must remain in history');
  assert.equal(view.recent[0].origin, 'recent');
}

{
  const view = buildConnectionView({
    activeSnapshot: [],
    recentSnapshot: [connection('reused-id', {
      revision: 2,
      state: 'completed',
      startedAtUnixMs: 1_000,
      endedAtUnixMs: 2_000,
    })],
    activeEvents: [connection('reused-id', {
      revision: 3,
      state: 'active',
      startedAtUnixMs: 3_000,
      updatedAtUnixMs: 3_100,
    })],
    recentEvents: [],
  });

  assert.equal(view.active.length, 1, 'a newer lifetime must remain active');
  assert.equal(view.recent.length, 0, 'an old same-id record must not replace the active lifetime');
}

{
  const view = buildConnectionView({
    activeSnapshot: [],
    recentSnapshot: [connection('kernel-history', {
      state: 'completed',
      endedAtUnixMs: 5_000,
      bytesDown: 42,
    })],
    activeEvents: [],
    recentEvents: [],
  });

  assert.equal(view.recent.length, 1, 'recent_flows must hydrate history without GUI events');
  assert.equal(view.recent[0].bytesDown, 42);
}

{
  const view = buildConnectionView({
    activeSnapshot: [connection('merge-update', {
      revision: 1,
      bytesDown: 10,
      selectionChain: ['selector', 'node-a'],
    })],
    recentSnapshot: [],
    activeEvents: [connection('merge-update', {
      revision: 2,
      bytesDown: 20,
      selectionChain: [],
      updatedAtUnixMs: 7_000,
    })],
    recentEvents: [],
  });

  assert.equal(view.active[0].bytesDown, 20, 'newer event counters must win');
  assert.deepEqual(
    view.active[0].selectionChain,
    ['selector', 'node-a'],
    'partial events must not erase a previously known selection chain',
  );
}

console.log('connection view reconciliation tests passed');
