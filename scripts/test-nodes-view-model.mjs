import assert from 'node:assert/strict';
import {
  buildRuntimeOverlay,
  buildSections,
  collectGroupNodeTags,
  filterNodes,
  normalizeSelectedGroup,
  sortNodes,
} from '../src/lib/components/tabs/nodes-view-model.ts';

function node(tag, delay, extra = {}) {
  return {
    id: tag,
    tag,
    name: tag,
    protocol: 'proxy',
    delay,
    domain: 'policy',
    ...extra,
  };
}

function group(name, outbounds, kind = 'selector') {
  return { name, kind, outbounds };
}

function tags(list) {
  return list.map((item) => item.tag);
}

function testDelaySortRanksReachableBeforeTimeoutAndUntested() {
  const sorted = sortNodes([
    node('timeout', -1),
    node('slow', 180),
    node('fresh', 42),
    node('untested', 0),
  ], 'delay');

  assert.deepEqual(tags(sorted), ['fresh', 'slow', 'timeout', 'untested']);
}

function testBuildSectionsKeepsOrphansWhenGroupsExist() {
  const sections = buildSections({
    allNodes: [node('HK', 30), node('JP', 70), node('orphan', 55)],
    groups: [group('Proxy', [{ tag: 'HK' }, { tag: 'JP' }])],
    query: '',
    sortMode: 'delay',
  });

  assert.equal(sections.length, 2);
  assert.equal(sections[0].name, 'Proxy');
  assert.deepEqual(tags(sections[0].nodes), ['HK', 'JP']);
  assert.equal(sections[1].name, '\u5176\u4ed6');
  assert.deepEqual(tags(sections[1].nodes), ['orphan']);
}

function testNestedGroupFilteringResolvesLeafNodes() {
  const groups = [
    group('Auto', [{ tag: 'Fallback' }, { tag: 'HK' }], 'urltest'),
    group('Fallback', [{ tag: 'JP' }, { tag: 'US' }], 'selector'),
  ];
  const nodes = [node('HK', 20), node('JP', 40), node('US', 60), node('SG', 10)];

  assert.deepEqual([...collectGroupNodeTags(groups, 'Auto')].sort(), ['HK', 'JP', 'US']);
  assert.deepEqual(tags(filterNodes({ allNodes: nodes, groups, query: '', selectedGroup: 'Auto', sortMode: 'delay' })), ['HK', 'JP', 'US']);
}

function testNormalizeSelectedGroupKeepsValidGroupAndClearsStaleValue() {
  const groups = [group('Auto', [{ tag: 'HK' }]), group('Fallback', [{ tag: 'JP' }])];

  assert.equal(normalizeSelectedGroup('Auto', groups), 'Auto');
  assert.equal(normalizeSelectedGroup('Missing', groups), null);
  assert.equal(normalizeSelectedGroup(null, groups), null);
}

function testRuntimeOverlayKeepsFirstGroupForSharedNodeTag() {
  const groups = [
    group('Primary', [{ tag: 'HK', delayMs: 30, alive: true }]),
    { ...group('Backup', [{ tag: 'HK', delayMs: 30, alive: true }]), selected: 'HK' },
  ];

  const overlay = buildRuntimeOverlay(groups);
  assert.deepEqual(overlay.get('HK'), {
    delayMs: 30,
    alive: true,
    selected: true,
    groupName: 'Primary',
  });
}

testDelaySortRanksReachableBeforeTimeoutAndUntested();
testBuildSectionsKeepsOrphansWhenGroupsExist();
testNestedGroupFilteringResolvesLeafNodes();
testNormalizeSelectedGroupKeepsValidGroupAndClearsStaleValue();
testRuntimeOverlayKeepsFirstGroupForSharedNodeTag();

console.log('nodes-view-model: ok');
