import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { EventLifecycleQueue } from '../src/lib/services/event-lifecycle.ts';

async function testLifecycleOperationsStayOrdered() {
  const queue = new EventLifecycleQueue();
  const calls = [];
  let releaseStart;
  const startGate = new Promise((resolve) => {
    releaseStart = resolve;
  });

  const start = queue.enqueue(async () => {
    calls.push('start:begin');
    await startGate;
    calls.push('start:end');
  });
  const stop = queue.enqueue(async () => {
    calls.push('stop');
  });

  await Promise.resolve();
  assert.deepEqual(calls, ['start:begin']);
  releaseStart();
  await Promise.all([start, stop]);
  assert.deepEqual(calls, ['start:begin', 'start:end', 'stop']);
}

async function testRejectedOperationDoesNotPoisonQueue() {
  const queue = new EventLifecycleQueue();
  await assert.rejects(queue.enqueue(async () => {
    throw new Error('start failed');
  }));

  let recovered = false;
  await queue.enqueue(async () => {
    recovered = true;
  });
  assert.equal(recovered, true);
}

async function testRootPageExclusivelyOwnsTheEventStream() {
  const [page, guiState] = await Promise.all([
    readFile(new URL('../src/routes/+page.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../src/lib/services/gui-state.svelte.ts', import.meta.url), 'utf8'),
  ]);

  assert.equal((page.match(/coreEvents\.start\(\)/g) ?? []).length, 1);
  assert.equal(guiState.includes('coreEvents.start()'), false);

  const inactiveBranch = page.slice(
    page.indexOf('if (!shouldInitialize)'),
    page.indexOf('void guiState.initialize()'),
  );
  assert.equal(inactiveBranch.includes('coreEvents.stop()'), false);
}

async function testCoreWarningsStayOutOfTransientNotifications() {
  const coreEvents = await readFile(
    new URL('../src/lib/services/core-events.svelte.ts', import.meta.url),
    'utf8',
  );
  const warningHandler = coreEvents.slice(
    coreEvents.indexOf('private _handleCoreWarning'),
    coreEvents.indexOf('private _handleCoreStatus'),
  );

  assert.equal(warningHandler.includes('this.warnings ='), true);
  assert.equal(warningHandler.includes('showWarningToast('), false);
}

await testLifecycleOperationsStayOrdered();
await testRejectedOperationDoesNotPoisonQueue();
await testRootPageExclusivelyOwnsTheEventStream();
await testCoreWarningsStayOutOfTransientNotifications();

console.log('core event lifecycle tests passed');
