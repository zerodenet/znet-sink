import assert from 'node:assert/strict';
import {
  buildAllNodes,
  buildRuntimeOverlay,
  buildSections,
  collectGroupNodeTags,
  filterNodes,
  getActiveNodeTag,
  mergePolicyGroups,
  normalizeSelectedGroup,
  planProbeTargets,
  policyProbeTagForNode,
  resolveProbeDisplay,
} from '../src/lib/components/tabs/nodes-view-model.ts';
import { buildPolicyProbeHistoryUpdates } from '../src/lib/services/policy-probe-history.ts';

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

function testRuntimeProbeTimestampOverridesOlderLocalHistory() {
  const checkedAt = 1_784_263_308_834;
  const groups = [{
    ...group('Auto', [{ tag: 'HK', delayMs: 133, alive: true, lastCheckedUnixMs: checkedAt }], 'url_test'),
    selected: 'HK',
  }];
  const runtimeOverlay = buildRuntimeOverlay(groups);
  const [hk] = buildAllNodes({
    configNodes: [{ tag: 'HK', protocol: 'shadowsocks', isSelector: false }],
    groups,
    runtimeOverlay,
    latestDelay: () => 154,
    latestProbeTime: () => checkedAt - 60 * 60 * 1000,
    fallbackNodes: [],
  });

  assert.equal(hk.delay, 133);
  assert.equal(hk.lastProbeAt, checkedAt);
}

function testNewerLocalProbeOverridesStaleRuntimeSnapshot() {
  assert.deepEqual(resolveProbeDisplay({
    runtimeDelay: 133,
    runtimeAt: 1_000,
    localDelay: 42,
    localAt: 2_000,
  }), { delay: 42, at: 2_000 });

  assert.deepEqual(resolveProbeDisplay({
    runtimeDelay: 31,
    runtimeAt: 3_000,
    localDelay: 42,
    localAt: 2_000,
  }), { delay: 31, at: 3_000 });
}

function testActiveNodeUsesCurrentlyBrowsedGroup() {
  const groups = [
    { ...group('Primary', [{ tag: 'HK' }, { tag: 'JP' }]), selected: 'HK' },
    { ...group('Auto', [{ tag: 'SG' }, { tag: 'US' }], 'url_test'), selected: 'SG' },
  ];

  assert.equal(getActiveNodeTag(groups, 'Auto'), 'SG');
  assert.equal(getActiveNodeTag(groups, 'Primary'), 'HK');
  assert.equal(getActiveNodeTag(groups, 'Missing'), undefined);
  assert.equal(getActiveNodeTag(groups), 'HK');
}

function testMergePolicyGroupsPreservesConfigAndAppliesRuntimeMemberState() {
  const config = [group('Auto', [{ tag: 'HK', type: 'vmess' }, { tag: 'JP', type: 'trojan' }], 'url_test')];
  const runtime = [{
    ...group('Auto', [
      { tag: 'HK', type: 'proxy', delayMs: 31, alive: true },
      { tag: 'JP', type: 'proxy', delayMs: 82, alive: false, lastError: 'timeout' },
    ], 'url_test'),
    selected: 'HK',
  }];

  const [merged] = mergePolicyGroups(config, runtime);
  assert.equal(merged.selected, 'HK');
  assert.deepEqual(merged.outbounds[0], { tag: 'HK', type: 'proxy', delayMs: 31, alive: true });
  assert.equal(merged.outbounds[1].lastError, 'timeout');
}

function testProbePlanningUsesKernelForUrlTestGroups() {
  const groups = [
    group('Proxy', [{ tag: 'HK' }, { tag: 'Auto' }]),
    group('Auto', [{ tag: 'JP' }, { tag: 'US' }], 'url_test'),
  ];
  const hk = node('HK', 20);
  const auto = node('Auto', 30, { protocol: 'url_test' });
  const jp = node('JP', 40);

  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Proxy', visibleNodes: [hk, auto] }), {
    nodes: [hk],
    policyTags: ['Auto'],
  });
  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Auto', visibleNodes: [jp] }), {
    nodes: [],
    policyTags: ['Auto'],
  });
}

function testProbePlanningTreatsNestedSelectorLikeARegularNode() {
  const groups = [
    group('Proxy', [{ tag: 'Fallback' }, { tag: 'HK' }]),
    group('Fallback', [{ tag: 'JP' }, { tag: 'US' }], 'selector'),
  ];
  const fallback = node('Fallback', 0, { protocol: 'selector' });
  const hk = node('HK', 20);

  assert.deepEqual(planProbeTargets({
    groups,
    selectedGroup: 'Proxy',
    visibleNodes: [fallback, hk],
  }), {
    nodes: [fallback, hk],
    policyTags: [],
  });
}

function testSingleCardProbeUsesPolicyOnlyForNestedUrlTestGroup() {
  const groups = [
    group('Proxy', [{ tag: 'Auto' }, { tag: 'HK' }]),
    group('Auto', [{ tag: 'JP' }, { tag: 'US' }], 'url_test'),
    group('Fallback', [{ tag: 'SG' }], 'selector'),
  ];

  assert.equal(policyProbeTagForNode(groups, 'Auto'), 'Auto');
  // Clicking a leaf while browsing Auto remains a single-node probe.
  assert.equal(policyProbeTagForNode(groups, 'JP'), undefined);
  // Other nested group types keep the ordinary outbound probe behavior.
  assert.equal(policyProbeTagForNode(groups, 'Fallback'), undefined);
}

function testPolicyProbeHistoryUsesSelectedResultFromSameScheduledEvent() {
  const completedAt = 1_784_294_531_466;
  const updates = buildPolicyProbeHistoryUpdates({
    policyTag: 'Auto-proxy',
    trigger: 'scheduled',
    completedAtUnixMs: completedAt,
    selected: 'ss-in',
    members: [
      { tag: 'ss-in', type: 'proxy', alive: true, delayMs: 794 },
      { tag: 'tr-sg', type: 'proxy', alive: true, delayMs: 815 },
    ],
  });

  assert.deepEqual(updates, [
    { tag: 'ss-in', delayMs: 794, reachable: true, at: completedAt },
    { tag: 'tr-sg', delayMs: 815, reachable: true, at: completedAt },
    {
      tag: 'Auto-proxy',
      delayMs: 794,
      reachable: true,
      at: completedAt,
      selectedTag: 'ss-in',
    },
  ]);
}

function testPolicyProbeHistoryDoesNotInventMissingSelectedResult() {
  const updates = buildPolicyProbeHistoryUpdates({
    policyTag: 'Auto-proxy',
    completedAtUnixMs: 2_000,
    selected: 'missing',
    members: [{ tag: 'ss-in', type: 'proxy', alive: true, delayMs: 30 }],
  });

  assert.deepEqual(updates, [
    { tag: 'ss-in', delayMs: 30, reachable: true, at: 2_000 },
  ]);
}

testBuildSectionsKeepsOrphansWhenGroupsExist();
testNestedGroupFilteringShowsNestedGroupAsMember();
testNormalizeSelectedGroupKeepsValidGroupAndClearsStaleValue();
testRuntimeOverlayKeepsFirstGroupForSharedNodeTag();
testRuntimeProbeTimestampOverridesOlderLocalHistory();
testNewerLocalProbeOverridesStaleRuntimeSnapshot();
testActiveNodeUsesCurrentlyBrowsedGroup();
testMergePolicyGroupsPreservesConfigAndAppliesRuntimeMemberState();
testProbePlanningUsesKernelForUrlTestGroups();
testProbePlanningTreatsNestedSelectorLikeARegularNode();
testSingleCardProbeUsesPolicyOnlyForNestedUrlTestGroup();
testPolicyProbeHistoryUsesSelectedResultFromSameScheduledEvent();
testPolicyProbeHistoryDoesNotInventMissingSelectedResult();

console.log('nodes-view-model: ok');
