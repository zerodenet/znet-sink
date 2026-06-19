import assert from 'node:assert/strict';
import { createBatchProbeState } from '../src/lib/components/tabs/nodes-probe-state.js';

function ids(set) {
  return [...set].sort();
}

function testAllNodesStartProbing() {
  const state = createBatchProbeState([
    { id: 'a', tag: 'hk' },
    { id: 'b', tag: 'sg' },
    { id: 'c', tag: 'jp' },
  ]);

  assert.deepEqual(ids(state.probingNodeIds()), ['a', 'b', 'c']);
}

function testResultClearsOnlyMatchingTag() {
  const state = createBatchProbeState([
    { id: 'row:hk', tag: 'hk' },
    { id: 'grid:hk', tag: 'hk' },
    { id: 'row:sg', tag: 'sg' },
  ]);

  assert.deepEqual(state.resolveTag('hk').sort(), ['grid:hk', 'row:hk']);
  assert.deepEqual(ids(state.probingNodeIds()), ['row:sg']);
}

function testCompletionClearsRemainingNodes() {
  const state = createBatchProbeState([
    { id: 'a', tag: 'hk' },
    { id: 'b', tag: 'sg' },
  ]);

  state.resolveTag('hk');
  assert.deepEqual(state.clear().sort(), ['b']);
  assert.deepEqual(ids(state.probingNodeIds()), []);
}

testAllNodesStartProbing();
testResultClearsOnlyMatchingTag();
testCompletionClearsRemainingNodes();

console.log('nodes-probe-state: ok');
