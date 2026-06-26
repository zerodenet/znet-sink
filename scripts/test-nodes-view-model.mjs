import assert from 'node:assert/strict';
import {
  buildRuntimeOverlay,
  buildSections,
  collectGroupNodeTags,
  filterNodes,
  normalizeSelectedGroup,
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

function testBuildSectionsKeepsOrphansWhenGroupsExist() {
  const sections = buildSections({
    allNodes: [node('HK', 30), node('JP', 70), node('orphan', 55)],
    groups: [group('Proxy', [{ tag: 'HK' }, { tag: 'JP' }])],
    query: '',
  });

  assert.equal(sections.length, 2);
  assert.equal(sections[0].name, 'Proxy');
  assert.deepEqual(tags(sections[0].nodes), ['HK', 'JP']);
  assert.equal(sections[1].name, '其他');
  assert.deepEqual(tags(sections[1].nodes), ['orphan']);
}

function testNestedGroupFilteringShowsNestedGroupAsMember() {
  // A nested group (Fallback inside Auto) stays as a direct member tag
  // rather than being expanded into JP/US, so it renders as a member card.
  const groups = [
    group('Auto', [{ tag: 'Fallback' }, { tag: 'HK' }], 'urltest'),
    group('Fallback', [{ tag: 'JP' }, { tag: 'US' }], 'selector'),
  ];
  const nodes = [
    node('HK', 20), node('JP', 40), node('US', 60), node('SG', 10),
    node('Fallback', 0, { protocol: 'selector' }),
  ];

  assert.deepEqual([...collectGroupNodeTags(groups, 'Auto')].sort(), ['Fallback', 'HK']);
  // Members render in group.outbounds order (Fallback before HK), not in
  // allNodes order (where the Fallback group card is appended at the tail).
  assert.deepEqual(tags(filterNodes({ allNodes: nodes, groups, query: '', selectedGroup: 'Auto' })), ['Fallback', 'HK']);
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

testBuildSectionsKeepsOrphansWhenGroupsExist();
testNestedGroupFilteringShowsNestedGroupAsMember();
testNormalizeSelectedGroupKeepsValidGroupAndClearsStaleValue();
testRuntimeOverlayKeepsFirstGroupForSharedNodeTag();

console.log('nodes-view-model: ok');
