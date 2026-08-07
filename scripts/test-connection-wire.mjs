import assert from 'node:assert/strict';
import {
  attachConnectionWireMetadata,
  buildConnectionWireIndex,
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
            flow_id: 'active-1',
            target: { host: 'example.com', port: 443 },
            timing: { started_at_unix_ms: 1_000 },
            extra_kernel_field: 'preserved',
          }],
        },
      },
    },
  };
  const index = buildConnectionWireIndex({ activeResponse });
  const enriched = attachConnectionWireMetadata(
    connection('active-1', { startedAtUnixMs: 1_000 }),
    index,
  );
  const rawPayload = enriched.rawPayload;

  assert.equal(enriched.rawSource, 'active_flows');
  assert.equal(rawPayload.extra_kernel_field, 'preserved');
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
          state: 'completed',
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
  assert.deepEqual(enriched.rawEnvelope, eventFrame.payload);
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
