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
    page.indexOf('const abortController = new AbortController()'),
  );
  assert.equal(inactiveBranch.includes('coreEvents.stop()'), false);
}

async function testProfileSwitchRotatesTheRuntimeEventGeneration() {
  const [configService, guiEventsCommand] = await Promise.all([
    readFile(new URL('../src/lib/services/config.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/src/commands/gui_events.rs', import.meta.url), 'utf8'),
  ]);
  const switchStart = configService.indexOf('export async function setActiveProxyConfig');
  const switchEnd = configService.indexOf('export async function removeProxyConfig', switchStart);
  const switchBody = configService.slice(switchStart, switchEnd);

  const activateIndex = switchBody.indexOf("invoke<ProxyConfigProfile>('proxy_config_set_active'");
  const stopIndex = switchBody.indexOf('await coreEvents.stop()');
  const startIndex = switchBody.indexOf('await coreEvents.start()');
  const signalIndex = switchBody.indexOf('proxyConfigSignal.markChanged(true)');

  assert.ok(configService.includes("import { coreEvents } from './core-events.svelte';"));
  assert.ok(activateIndex >= 0);
  assert.ok(stopIndex > activateIndex);
  assert.ok(startIndex > stopIndex);
  assert.ok(signalIndex > startIndex);

  const stopCommandStart = guiEventsCommand.indexOf('pub fn gui_events_stop');
  const stopCommandEnd = guiEventsCommand.indexOf('fn resolve_options', stopCommandStart);
  const stopCommand = guiEventsCommand.slice(stopCommandStart, stopCommandEnd);
  assert.ok(guiEventsCommand.includes('use crate::kernel::connection;'));
  assert.ok(stopCommand.includes('state.next_gui_event_generation()'));
  assert.ok(stopCommand.includes('connection::reset();'));
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

async function testRetiredIpcConnectionsRotateEventSubscriptions() {
  const [connection, guiEvents, coreEvents] = await Promise.all([
    readFile(new URL('../src-tauri/src/kernel/connection.rs', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/src/services/gui_events.rs', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/src/services/core_events.rs', import.meta.url), 'utf8'),
  ]);

  assert.ok(connection.includes('pub(crate) fn retire(&self)'));
  assert.ok(guiEvents.includes('if !conn.is_alive()'));
  assert.ok(guiEvents.includes('conn.retire();'));
  assert.ok(guiEvents.includes('conn.has_pending_requests()'));
  assert.ok(guiEvents.includes('conn.received_within('));
  assert.ok(coreEvents.includes('if !conn.is_alive()'));
  assert.equal(coreEvents.includes('receiver.blocking_recv()'), false);
  assert.ok(connection.includes('conn.received_within(activity_window) || conn.has_pending_requests()'));
}

await testLifecycleOperationsStayOrdered();
await testRejectedOperationDoesNotPoisonQueue();
await testRootPageExclusivelyOwnsTheEventStream();
await testProfileSwitchRotatesTheRuntimeEventGeneration();
await testCoreWarningsStayOutOfTransientNotifications();
await testRetiredIpcConnectionsRotateEventSubscriptions();

console.log('core event lifecycle tests passed');
