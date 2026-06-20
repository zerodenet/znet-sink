import assert from 'node:assert/strict';
import { createNodesProbeController } from '../src/lib/components/tabs/nodes-probe-controller.js';

function createListenerRegistry() {
  const handlers = new Map();
  return {
    async listen(event, handler) {
      handlers.set(event, handler);
      return () => handlers.delete(event);
    },
    async emit(event, payload) {
      const handler = handlers.get(event);
      if (!handler) return;
      await handler({ payload });
    },
  };
}

async function testSingleProbeLifecycle() {
  const states = [];
  let releaseProbe;
  let refreshCalls = 0;

  const controller = createNodesProbeController({
    listen: async () => () => {},
    probeNode: () =>
      new Promise((resolve) => {
        releaseProbe = () => resolve({ targetTag: 'A', reachable: true, latencyMs: 42 });
      }),
    probeAll: async () => {},
    recordDelay: () => {},
    refreshPolicyGroups: async () => {
      refreshCalls += 1;
    },
    onStateChange: (state) => states.push(state),
  });

  const probePromise = controller.handleProbe({ id: 'node-1', tag: 'A' });
  assert.equal(states.at(-1)?.probingNodeIds.has('node-1'), true);

  releaseProbe();
  await probePromise;

  assert.equal(states.at(-1)?.probingNodeIds.size, 0);
  assert.equal(states.at(-1)?.probingAll, false);
  assert.equal(refreshCalls, 1);
}

async function testBatchProbeLifecycle() {
  const states = [];
  const seenDelays = [];
  let refreshCalls = 0;
  let batchSessionId;
  const registry = createListenerRegistry();

  const controller = createNodesProbeController({
    listen: registry.listen,
    probeNode: async () => ({ targetTag: 'unused', reachable: true }),
    probeAll: async (_targetTags, sessionId) => {
      batchSessionId = sessionId;
      await registry.emit('probe:result', { sessionId: 'stale-session', targetTag: 'A', reachable: true, latencyMs: 999 });
      await registry.emit('probe:progress', { sessionId: 'stale-session', done: 99, total: 99 });
      await registry.emit('probe:complete', { sessionId: 'stale-session', total: 99, reachable: 99, failed: 0 });
      await registry.emit('probe:result', { sessionId, targetTag: 'A', reachable: true, latencyMs: 12 });
      await registry.emit('probe:progress', { sessionId, done: 1, total: 2 });
      await registry.emit('probe:result', { sessionId, targetTag: 'B', reachable: false });
      await registry.emit('probe:progress', { sessionId, done: 2, total: 2 });
      await registry.emit('probe:complete', { sessionId, total: 2, reachable: 1, failed: 1 });
    },
    recordDelay: (targetTag, latencyMs, reachable) => {
      seenDelays.push({ targetTag, latencyMs, reachable });
    },
    refreshPolicyGroups: async () => {
      refreshCalls += 1;
    },
    onStateChange: (state) => states.push(state),
  });

  await controller.handleProbeAll([
    { id: 'node-1', tag: 'A' },
    { id: 'node-2', tag: 'B' },
  ]);

  assert.equal(states[0].probingAll, true);
  assert.equal(typeof batchSessionId, 'string');
  assert.ok(batchSessionId.length > 0);
  assert.deepEqual([...states[0].probingNodeIds].sort(), ['node-1', 'node-2']);
  assert.equal(states.some((state) => state.probingNodeIds.size === 1), true);
  assert.deepEqual(states.at(-1).probeProgress, { done: 2, total: 2 });
  assert.equal(states.at(-1).probingAll, false);
  assert.equal(states.at(-1).probingNodeIds.size, 0);
  assert.deepEqual(seenDelays, [
    { targetTag: 'A', latencyMs: 12, reachable: true },
    { targetTag: 'B', latencyMs: undefined, reachable: false },
  ]);
  assert.equal(refreshCalls, 1);
}

async function testEmptyBatchProbeIsNoOp() {
  const states = [];
  let probeAllCalls = 0;

  const controller = createNodesProbeController({
    listen: async () => () => {},
    probeNode: async () => ({ targetTag: 'unused', reachable: true }),
    probeAll: async () => {
      probeAllCalls += 1;
    },
    recordDelay: () => {},
    refreshPolicyGroups: async () => {},
    onStateChange: (state) => states.push(state),
  });

  await controller.handleProbeAll([]);

  assert.equal(probeAllCalls, 0);
  assert.deepEqual(states.at(-1), {
    probingNodeIds: new Set(),
    probingAll: false,
    probeProgress: { done: 0, total: 0 },
    lastError: null,
  });
}

async function testBatchProbeDoesNotStartWhileSingleProbeIsRunning() {
  const states = [];
  let probeAllCalls = 0;
  let releaseProbe;

  const controller = createNodesProbeController({
    listen: async () => () => {},
    probeNode: () =>
      new Promise((resolve) => {
        releaseProbe = () => resolve({ targetTag: 'A', reachable: true, latencyMs: 42 });
      }),
    probeAll: async () => {
      probeAllCalls += 1;
    },
    recordDelay: () => {},
    refreshPolicyGroups: async () => {},
    onStateChange: (state) => states.push(state),
  });

  const singleProbe = controller.handleProbe({ id: 'node-1', tag: 'A' });
  await controller.handleProbeAll([
    { id: 'node-1', tag: 'A' },
    { id: 'node-2', tag: 'B' },
  ]);

  assert.equal(probeAllCalls, 0);
  assert.equal(states.at(-1)?.probingAll, false);
  assert.deepEqual([...states.at(-1)?.probingNodeIds ?? []], ['node-1']);

  releaseProbe();
  await singleProbe;
}

await testSingleProbeLifecycle();
await testBatchProbeLifecycle();
await testEmptyBatchProbeIsNoOp();
await testBatchProbeDoesNotStartWhileSingleProbeIsRunning();

console.log('nodes-probe-controller: ok');
