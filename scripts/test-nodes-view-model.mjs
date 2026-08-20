import assert from 'node:assert/strict';

// The view model imports a Svelte rune-backed preference store. Unit tests run
// the TypeScript module directly without the Svelte compiler, so provide the
// identity behavior needed for its initial scalar state.
globalThis.$state = (value) => value;

const {
  buildSections,
  collectProbingPolicyNodeTags,
  filterNodes,
  getActiveNodeTag,
  normalizeSelectedGroup,
  planProbeTargets,
  projectNestedGroupNodes,
  resolveEffectiveNodeSelection,
  summarizeProbeProgress,
} = await import('../src/lib/components/tabs/nodes-view-model.ts');
const { nodesDisplayPreferences } = await import(
  '../src/lib/components/tabs/nodes-display-preferences.svelte.ts'
);
import { flagCodeFromEmoji, parseNodeName } from '../src/lib/services/node-utils.ts';

const node = (tag, protocol = 'proxy') => ({
  id: tag,
  tag,
  name: tag,
  protocol,
  delay: 0,
  domain: 'default',
});
const group = (name, tags, kind = 'selector', selected) => ({
  name,
  kind,
  selected,
  outbounds: tags.map((tag) => ({ tag, type: 'proxy' })),
});

{
  assert.equal(flagCodeFromEmoji('🇯🇵'), 'JP');
  assert.equal(flagCodeFromEmoji('🚀'), undefined);
  assert.deepEqual(parseNodeName('🇯🇵 日本 IEPL'), {
    emoji: '🇯🇵',
    flagCode: 'JP',
    cleanName: '日本 IEPL',
  });
  assert.deepEqual(parseNodeName('🚀 Fast'), {
    emoji: '🚀',
    cleanName: 'Fast',
  });
}

{
  const groups = [group('Auto', ['HK', 'JP'], 'url_test'), group('Manual', ['US'])];
  assert.deepEqual([...collectProbingPolicyNodeTags(groups, new Set(['Auto']))], ['Auto']);
}

{
  const groups = [group('Proxy', ['HK', 'Auto']), group('Auto', ['JP', 'US'], 'url_test')];
  const hk = node('HK');
  const auto = node('Auto', 'url_test');
  const jp = node('JP');
  const us = node('US');

  // A nested URLTest group remains one effective outbound card in its parent
  // group. Probing the card measures its current selected route; it does not
  // implicitly start a full URLTest policy refresh.
  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Proxy', visibleNodes: [hk, auto] }), {
    nodes: [hk, auto],
  });

  // Once the URLTest group is opened, its visible direct members remain
  // independent outbound targets.
  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Auto', visibleNodes: [jp, us] }), {
    nodes: [jp, us],
  });

  // The global view follows the same visible-card semantics.
  assert.deepEqual(planProbeTargets({
    groups,
    selectedGroup: null,
    visibleNodes: [hk, auto, jp, us],
  }), {
    nodes: [hk, auto, jp, us],
  });

  // Built-in outbounds are not filtered by the GUI; the kernel owns the
  // diagnostic outcome for direct, reject, DNS, pass, and similar targets.
  const direct = node('DIRECT', 'direct');
  const reject = node('REJECT', 'reject');
  assert.deepEqual(planProbeTargets({
    groups,
    selectedGroup: 'Proxy',
    visibleNodes: [direct, reject],
  }), {
    nodes: [direct, reject],
  });
}

{
  const groups = [
    group('Proxy', ['HK', 'Auto']),
    group('Auto', ['JP', 'Nested', 'JP'], 'url_test'),
    group('Nested', ['US', 'SG'], 'url_test'),
  ];
  const runningNestedOutbound = {
    id: 0,
    kind: 'outbound',
    state: 'running',
    targetTags: ['Auto'],
    results: [],
  };
  assert.deepEqual(summarizeProbeProgress(groups, [runningNestedOutbound]), { done: 0, total: 1 });
  assert.deepEqual(summarizeProbeProgress(groups, [{
    ...runningNestedOutbound,
    results: [{ targetTag: 'Auto', reachable: true }],
  }]), { done: 1, total: 1 });

  const runningPolicy = {
    id: 1,
    kind: 'manual_policy',
    state: 'running',
    targetTags: ['Auto'],
    results: [],
  };
  assert.deepEqual(summarizeProbeProgress(groups, [runningPolicy]), { done: 0, total: 3 });

  const completedPolicy = {
    ...runningPolicy,
    results: [{ targetTag: 'Auto', reachable: true }],
  };
  assert.deepEqual(summarizeProbeProgress(groups, [completedPolicy]), { done: 3, total: 3 });

  const mixedJobs = [
    runningPolicy,
    {
      id: 2,
      kind: 'outbound',
      state: 'running',
      targetTags: ['JP', 'HK'],
      results: [{ targetTag: 'HK', reachable: true }],
    },
  ];
  assert.deepEqual(summarizeProbeProgress(groups, mixedJobs), { done: 1, total: 4 });
}

{
  const groups = [group('Proxy', ['HK', 'JP'], 'selector', 'JP')];
  assert.deepEqual(filterNodes({ allNodes: [node('JP'), node('HK')], groups, query: '', selectedGroup: 'Proxy' }).map((item) => item.tag), ['HK', 'JP']);
  assert.equal(getActiveNodeTag(groups, 'Proxy'), 'JP');
  assert.equal(normalizeSelectedGroup('missing', groups), null);
}

{
  const nodes = [
    { ...node('untested-a'), delay: 0 },
    { ...node('slow'), delay: 120, lastProbeAt: 1_000, alive: true },
    { ...node('timeout'), delay: -1, lastProbeAt: 1_000, alive: false },
    { ...node('fast'), delay: 35, lastProbeAt: 1_000, alive: true },
    { ...node('untested-b'), delay: 0 },
    { ...node('failed'), delay: 0, lastProbeAt: 1_000, alive: false },
  ];
  const groups = [group('Auto', nodes.map((item) => item.tag), 'urltest')];

  assert.deepEqual(filterNodes({
    allNodes: nodes,
    groups,
    query: '',
    selectedGroup: 'Auto',
  }).map((item) => item.tag), nodes.map((item) => item.tag));

  nodesDisplayPreferences.setSortByDelay(true);
  assert.deepEqual(filterNodes({
    allNodes: nodes,
    groups,
    query: '',
    selectedGroup: 'Auto',
  }).map((item) => item.tag), [
    'fast',
    'slow',
    'untested-a',
    'untested-b',
    'timeout',
    'failed',
  ]);
  nodesDisplayPreferences.setSortByDelay(false);
}

{
  const nodes = [
    { ...node('slow'), delay: 90, lastProbeAt: 1_000, alive: true },
    { ...node('fast'), delay: 20, lastProbeAt: 1_000, alive: true },
  ];
  const groups = [
    group('Auto', ['slow', 'fast'], 'url_test'),
    group('Manual', ['slow', 'fast'], 'selector'),
    group('Fallback', ['slow', 'fast'], 'fallback'),
  ];
  nodesDisplayPreferences.setSortByDelay(true);
  const sections = buildSections({ allNodes: nodes, groups, query: '' });

  assert.deepEqual(sections[0].nodes.map((item) => item.tag), ['fast', 'slow']);
  assert.deepEqual(filterNodes({
    allNodes: nodes,
    groups,
    query: '',
    selectedGroup: 'Manual',
  }).map((item) => item.tag), ['slow', 'fast']);
  assert.deepEqual(filterNodes({
    allNodes: nodes,
    groups,
    query: '',
    selectedGroup: 'Fallback',
  }).map((item) => item.tag), ['slow', 'fast']);
  nodesDisplayPreferences.setSortByDelay(false);
}

{
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['Nested'], 'url_test', 'Nested'),
    group('Nested', ['JP', 'US'], 'url_test', 'JP'),
  ];
  assert.deepEqual(resolveEffectiveNodeSelection(groups, 'Proxy'), {
    leafTag: 'JP',
    groupPath: ['Proxy', 'Auto', 'Nested'],
    leafParentKind: 'url_test',
    cycleDetected: false,
  });
}

{
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['JP', 'US'], 'url_test'),
  ];
  assert.deepEqual(resolveEffectiveNodeSelection(groups, 'Proxy'), {
    groupPath: ['Proxy', 'Auto'],
    leafParentKind: 'url_test',
    unresolvedGroupTag: 'Auto',
    cycleDetected: false,
  });
}

{
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['Proxy'], 'url_test', 'Proxy'),
  ];
  assert.deepEqual(resolveEffectiveNodeSelection(groups, 'Proxy'), {
    groupPath: ['Proxy', 'Auto'],
    leafParentKind: 'url_test',
    unresolvedGroupTag: 'Proxy',
    cycleDetected: true,
  });
}

{
  const sections = buildSections({
    allNodes: [node('HK'), node('orphan')],
    groups: [group('Proxy', ['HK'])],
    query: '',
  });
  assert.deepEqual(sections.map((section) => [section.name, section.nodes.map((item) => item.tag)]), [
    ['Proxy', ['HK']],
    ['其他', ['orphan']],
  ]);
}

{
  const hk = { ...node('HK'), delay: 30, lastProbeAt: 1_000, alive: true };
  const us = { ...node('US'), delay: 80, lastProbeAt: 2_000, alive: true };
  const auto = { ...node('Auto', 'url_test'), delay: 30, lastProbeAt: 1_000, alive: true };
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['HK', 'US'], 'url_test', 'US'),
  ];

  const [projectedAuto] = filterNodes({
    allNodes: [hk, us, auto],
    groups,
    query: '',
    selectedGroup: 'Proxy',
  });
  assert.equal(projectedAuto.tag, 'Auto');
  assert.equal(projectedAuto.protocol, 'url_test');
  assert.equal(projectedAuto.delay, 80);
  assert.equal(projectedAuto.lastProbeAt, 2_000);
  assert.equal(projectedAuto.alive, true);
}

{
  const hk = { ...node('HK'), delay: 30, lastProbeAt: 1_000, alive: true };
  const us = { ...node('US'), delay: 0, lastProbeAt: undefined, alive: undefined };
  const auto = { ...node('Auto', 'url_test'), delay: 30, lastProbeAt: 1_000, alive: true };
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['HK', 'US'], 'url_test', 'US'),
  ];

  const [projectedAuto] = projectNestedGroupNodes([hk, us, auto], groups)
    .filter((item) => item.tag === 'Auto');
  assert.equal(projectedAuto.delay, 0);
  assert.equal(projectedAuto.lastProbeAt, undefined);
  assert.equal(projectedAuto.alive, undefined);
}

{
  const us = { ...node('US'), delay: 64, lastProbeAt: 3_000, alive: true };
  const nested = { ...node('Nested', 'url_test'), delay: 40, lastProbeAt: 1_500, alive: true };
  const auto = { ...node('Auto', 'url_test'), delay: 35, lastProbeAt: 1_000, alive: true };
  const groups = [
    group('Proxy', ['Auto'], 'selector', 'Auto'),
    group('Auto', ['Nested'], 'url_test', 'Nested'),
    group('Nested', ['US'], 'url_test', 'US'),
  ];

  const projected = projectNestedGroupNodes([us, nested, auto], groups);
  assert.equal(projected.find((item) => item.tag === 'Auto')?.delay, 64);
  assert.equal(projected.find((item) => item.tag === 'Nested')?.delay, 64);
}

console.log('nodes-view-model: ok');
