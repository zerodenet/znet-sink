import assert from 'node:assert/strict';
import {
  buildSections,
  collectProbingPolicyNodeTags,
  filterNodes,
  getActiveNodeTag,
  normalizeSelectedGroup,
  planProbeTargets,
  policyProbeTagForNode,
} from '../src/lib/components/tabs/nodes-view-model.ts';

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
  const groups = [group('Auto', ['HK', 'JP'], 'url_test'), group('Manual', ['US'])];
  assert.deepEqual([...collectProbingPolicyNodeTags(groups, new Set(['Auto']))], ['Auto']);
  assert.equal(policyProbeTagForNode(groups, 'Auto'), 'Auto');
  assert.equal(policyProbeTagForNode(groups, 'HK'), undefined);
}

{
  const groups = [group('Proxy', ['HK', 'Auto']), group('Auto', ['JP', 'US'], 'url_test')];
  const hk = node('HK');
  const auto = node('Auto', 'url_test');
  const jp = node('JP');
  const us = node('US');

  // A nested URLTest group remains one effective card in its parent group.
  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Proxy', visibleNodes: [hk, auto] }), {
    nodes: [hk],
    policyTags: ['Auto'],
  });

  // Once the URLTest group is opened, its visible direct members are probed
  // independently and progress reflects the actual cards instead of 0/1.
  assert.deepEqual(planProbeTargets({ groups, selectedGroup: 'Auto', visibleNodes: [jp, us] }), {
    nodes: [jp, us],
    policyTags: [],
  });

  // The global view follows the same visible-card semantics: the nested group
  // card and each separately visible member are counted independently.
  assert.deepEqual(planProbeTargets({
    groups,
    selectedGroup: null,
    visibleNodes: [hk, auto, jp, us],
  }), {
    nodes: [hk, jp, us],
    policyTags: ['Auto'],
  });
}

{
  const groups = [group('Proxy', ['HK', 'JP'], 'selector', 'JP')];
  assert.deepEqual(filterNodes({ allNodes: [node('JP'), node('HK')], groups, query: '', selectedGroup: 'Proxy' }).map((item) => item.tag), ['HK', 'JP']);
  assert.equal(getActiveNodeTag(groups, 'Proxy'), 'JP');
  assert.equal(normalizeSelectedGroup('missing', groups), null);
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

console.log('nodes-view-model: ok');
