import assert from 'node:assert/strict';
import { ConnectionDeltaBatcher } from '../src/lib/services/connection-delta-batcher.ts';

class FakeScheduler {
  callbacks = new Map();
  nextId = 1;
  scheduleCount = 0;

  setTimeout(callback) {
    const id = this.nextId++;
    this.callbacks.set(id, callback);
    this.scheduleCount++;
    return id;
  }

  clearTimeout(id) {
    this.callbacks.delete(id);
  }

  runNext() {
    const entry = this.callbacks.entries().next().value;
    assert.ok(entry, 'expected a scheduled flush');
    const [id, callback] = entry;
    this.callbacks.delete(id);
    callback();
  }
}

{
  const scheduler = new FakeScheduler();
  const batches = [];
  const batcher = new ConnectionDeltaBatcher(300, (items) => batches.push(items), scheduler);

  batcher.push(['started']);
  batcher.push(['updated-1']);
  batcher.push(['updated-2']);

  assert.equal(scheduler.scheduleCount, 1, 'new deltas must not postpone the scheduled flush');
  assert.equal(batches.length, 0);

  scheduler.runNext();
  assert.deepEqual(batches, [['started', 'updated-1', 'updated-2']]);

  batcher.push(['updated-3']);
  assert.equal(scheduler.scheduleCount, 2, 'the next interval must schedule independently');
  scheduler.runNext();
  assert.deepEqual(batches[1], ['updated-3']);
}

{
  const scheduler = new FakeScheduler();
  const batches = [];
  const batcher = new ConnectionDeltaBatcher(300, (items) => batches.push(items), scheduler);
  batcher.push(['pending']);
  batcher.destroy();
  assert.equal(scheduler.callbacks.size, 0, 'destroy must cancel the pending flush');
  assert.deepEqual(batches, []);
}

console.log('connection delta batcher tests passed');
