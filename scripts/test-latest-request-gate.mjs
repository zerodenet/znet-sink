import assert from 'node:assert/strict';
import { createLatestRequestGate } from '../src/lib/services/latest-request-gate.js';

const gate = createLatestRequestGate();

const firstGeneration = gate.reset();
const first = gate.begin(firstGeneration);
const second = gate.begin(firstGeneration);

assert.equal(gate.canApply(second), true, 'newer response should apply');
assert.equal(gate.canApply(first), false, 'late older response must not overwrite newer data');
assert.equal(gate.isLatest(second), true, 'latest request owns completion state');
assert.equal(gate.isLatest(first), false, 'older request must not clear the loading state');

const secondGeneration = gate.reset();
const current = gate.begin(secondGeneration);

assert.equal(gate.canApply(second), false, 'filter/query changes invalidate old generations');
assert.equal(gate.isCurrentGeneration(firstGeneration), false);
assert.equal(gate.canApply(current), true);

console.log('latest request gate tests passed');
