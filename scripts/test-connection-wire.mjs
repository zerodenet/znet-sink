import assert from 'node:assert/strict';
import {
  attachConnectionWireMetadata,
  buildConnectionWireIndex,
  mergeConnectionWireIndexes,
} from '../src/lib/services/connection-wire.ts';

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
  const activeResponse = {
    available: true,
    response: {
      ok: true,
      result: {
        active_flows: {
          items: [{
            id: 101,
            target: { host: 'example.com', port: 443 },
            started_at_unix_ms: 1_000,
            last_activity_at_unix_ms: 1_500,
            throughput_up_bps: 120,
            throughput_down_bps: 340,
            extra_kernel_field: 'preserved',
          }],
        },
      },
    },
  };
  const index = buildConnectionWireIndex({ activeResponse });
  const enriched = attachConnectionWireMetadata(
    connection('101', {
      startedAtUnixMs: 1_000,
      throughputUpBps: null,
      throughputDownBps: null,
    }),
    index,
  );
  const rawPayload = enriched.rawPayload;

  assert.equal(enriched.rawSource, 'active_flows');
  assert.equal(rawPayload.extra_kernel_field, 'preserved');
  assert.equal(enriched.throughputUpBps, 120, 'numeric query ids must attach raw throughput');
  assert.equal(enriched.throughputDownBps, 340);
  assert.equal(enriched.lastActivityAtUnixMs, 1_500);
}

{
  const eventFrame = {
    id: 12,
    atMs: 9_999,
    direction: 'rx',
    frameType: 'event',
    payload: {
      event_type: 'flow.completed',
      event_id: 'evt-12',
      sequence: 44,
      occurred_at_unix_ms: 8_000,
      payload: {
        record: {
          flow_id: 'completed-1',
          revision: 7,
          state: 'completed',
          source: {
            ip: '127.0.0.1',
            port: 52_000,
            process_name: 'browser.exe',
          },
          throughput: {
            upload_bps: 12,
            download_bps: 34,
            sampled_at_unix_ms: 8_000,
          },
          timing: {
            started_at_unix_ms: 2_000,
            ended_at_unix_ms: 8_000,
          },
          result: { outcome: 'success' },
        },
      },
    },
  };
  const index = buildConnectionWireIndex({ eventFrames: [eventFrame] });
  const enriched = attachConnectionWireMetadata(
    connection('completed-1', {
      state: 'completed',
      source: null,
      revision: null,
      startedAtUnixMs: 2_000,
      endedAtUnixMs: 8_000,
    }),
    index,
  );

  assert.equal(enriched.rawSource, 'event');
  assert.equal(enriched.eventType, 'flow.completed');
  assert.equal(enriched.eventId, 'evt-12');
  assert.equal(enriched.eventSequence, 44);
  assert.equal(enriched.eventOccurredAtUnixMs, 8_000);
  assert.equal(enriched.source, '127.0.0.1:52000');
  assert.equal(enriched.processName, 'browser.exe');
  assert.equal(enriched.revision, 7);
  assert.equal(enriched.throughputUpBps, 12);
  assert.equal(enriched.throughputDownBps, 34);
  assert.equal(enriched.updatedAtUnixMs, 8_000);
  assert.deepEqual(enriched.rawEnvelope, eventFrame.payload);

  const repeated = mergeConnectionWireIndexes(index, index);
  assert.equal(repeated['completed-1'].length, 1, 're-reading debug frames must not duplicate wire records');
}

{
  const index = buildConnectionWireIndex({
    eventFrames: [
      {
        id: 1,
        atMs: 2_000,
        direction: 'rx',
        frameType: 'event',
        payload: {
          event_type: 'flow.completed',
          occurred_at_unix_ms: 2_000,
          payload: { record: { flow_id: 'reused', timing: { started_at_unix_ms: 1_000, ended_at_unix_ms: 2_000 } } },
        },
      },
      {
        id: 2,
        atMs: 4_000,
        direction: 'rx',
        frameType: 'event',
        payload: {
          event_type: 'flow.started',
          occurred_at_unix_ms: 4_000,
          payload: { record: { flow_id: 'reused', timing: { started_at_unix_ms: 4_000 } } },
        },
      },
    ],
  });

  const oldLifetime = attachConnectionWireMetadata(
    connection('reused', { state: 'completed', startedAtUnixMs: 1_000, endedAtUnixMs: 2_000 }),
    index,
  );
  const newLifetime = attachConnectionWireMetadata(
    connection('reused', { state: 'active', startedAtUnixMs: 4_000 }),
    index,
  );

  assert.equal(oldLifetime.eventType, 'flow.completed');
  assert.equal(newLifetime.eventType, 'flow.started');
}

console.log('connection wire metadata tests passed');
