import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CaptureLedger, overviewCounters, trafficShare } from '../src/lib/features/mobile/overview.ts';
const snapshot = (up, down, at, extra = {}) => ({
  scope: {kind: 'global'}, core_instance_id: 'instance-a', config_revision: '1', generation: null,
  stats_epoch: 'period-a', epoch_started_at_unix_ms: '1', capture_started_at_unix_ms: '1',
  sampled_at_monotonic_ns: String(at), sampled_at_unix_ms: String(at),
  planes: [{plane: 'flow', accounting_basis: 'logical', source_roles: [],
    counters: {bytes_up: String(up), bytes_down: String(down)}, available_metrics: ['bytes_up', 'bytes_down'], resettable_metrics: []}],
  activity: {}, reset_policy: 'none', ...extra,
});
test('capture uses exact u64 deltas and does not count pre-existing cumulative traffic', () => {
  const ledger = new CaptureLedger();
  const start = 9007199254740993n;
  ledger.update(true, snapshot(start, start, 1));
  assert.equal(ledger.up, 0n);
  ledger.update(true, snapshot(start + 17n, start + 29n, 2));
  assert.equal(ledger.up, 17n); assert.equal(ledger.down, 29n);
  ledger.update(false, snapshot(start + 100n, start + 100n, 3));
  assert.equal(ledger.up, 17n);
  ledger.update(true, undefined); assert.equal(ledger.up, null);
});
test('instance, configuration, epoch and observation gap require new baselines', () => {
  for (const extra of [{core_instance_id:'instance-b'}, {config_revision:'2'}, {stats_epoch:'period-b'}]) {
    const ledger = new CaptureLedger();
    ledger.update(true, snapshot(100, 100, 1));
    ledger.update(true, snapshot(110, 120, 2));
    ledger.update(true, snapshot(400, 800, 3, extra));
    assert.equal(ledger.up, 10n); assert.equal(ledger.down, 20n);
    ledger.update(true, snapshot(405, 807, 4, extra));
    assert.equal(ledger.up, 15n); assert.equal(ledger.down, 27n);
    ledger.interrupt(); ledger.update(true, snapshot(600, 900, 5, extra));
    assert.equal(ledger.up, 15n);
  }
});
test('missing counters, late samples and a reset never manufacture bytes or zero metrics', () => {
  const ledger = new CaptureLedger();
  ledger.update(true, snapshot(100,100,5));
  ledger.update(true, snapshot(90,90,4));
  ledger.update(true, snapshot(105,107,6));
  assert.equal(ledger.up,5n);
  ledger.update(true, snapshot(1,1,7));
  assert.equal(ledger.up,5n);
  const missing = snapshot(0,0,8,{planes:[]});
  ledger.update(true,missing);
  assert.equal(overviewCounters({snapshot:missing}).total,null);
  ledger.update(true,snapshot(100,100,9));
  assert.equal(ledger.up,5n);
  assert.equal(trafficShare(null,100n),null); assert.equal(trafficShare(0n,0n),null);
  assert.equal(trafficShare(9007199254740993n,9007199254740993n),50);
});
