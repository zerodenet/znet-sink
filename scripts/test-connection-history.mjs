import assert from 'node:assert/strict';
import { buildPersistedConnectionHistory } from '../src/lib/services/connection-history.ts';

const completed = {
  id: 2,
  atMs: 9_100,
  direction: 'rx',
  frameType: 'event',
  payload: {
    event_type: 'flow.completed',
    event_id: 'completed-1',
    sequence: 9,
    occurred_at_unix_ms: 9_000,
    payload: {
      record: {
        flow_id: '42',
        revision: 4,
        state: 'completed',
        network: 'tcp',
        source: {
          ip: '127.0.0.1',
          port: 52_000,
          process_name: 'browser.exe',
        },
        target: {
          host: 'example.com',
          port: 443,
          resolved_ip: '203.0.113.8',
        },
        inbound: { tag: 'mixed-in', protocol: 'mixed' },
        route: {
          mode: 'rule',
          action: 'proxy',
          selection_chain: ['auto', 'node-a'],
        },
        path: {
          outbound: { tag: 'node-a', protocol: 'vless' },
          relay_chain: [],
        },
        traffic: { bytes_up: 120, bytes_down: 340 },
        throughput: {
          upload_bps: 12,
          download_bps: 34,
          sampled_at_unix_ms: 9_000,
        },
        timing: {
          started_at_unix_ms: 2_000,
          last_activity_at_unix_ms: 8_900,
          ended_at_unix_ms: 9_000,
          duration_ms: 7_000,
        },
        result: { outcome: 'direct_relayed', close_reason: 'eof' },
      },
    },
  },
};

const updated = {
  id: 1,
  atMs: 8_000,
  direction: 'rx',
  frameType: 'event',
  payload: {
    event_type: 'flow.updated',
    occurred_at_unix_ms: 8_000,
    payload: { record: { flow_id: '42' } },
  },
};

const history = buildPersistedConnectionHistory([updated, completed, completed]);

assert.equal(history.length, 1, 'only completed lifecycle records should be restored and duplicates removed');
assert.equal(history[0].flowId, '42');
assert.equal(history[0].source, '127.0.0.1:52000');
assert.equal(history[0].processName, 'browser.exe');
assert.equal(history[0].destination, 'example.com:443');
assert.equal(history[0].outboundTag, 'node-a');
assert.equal(history[0].bytesUp, 120);
assert.equal(history[0].throughputDownBps, 34);
assert.equal(history[0].eventSequence, 9);
assert.deepEqual(history[0].rawEnvelope, completed.payload);

console.log('local connection history recovery tests passed');
