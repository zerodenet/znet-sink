import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replaceAll('\r\n', '\n');
}

function assertUsesTag(path, expectedSnippet) {
  const content = read(path);
  assert.ok(
    content.includes(expectedSnippet),
    `${path} should select nodes using outbound tag`,
  );
  assert.ok(
    !content.includes('selectPolicy(\'proxy\', node.name)') &&
      !content.includes('selectPolicy(policyTag, node.name)'),
    `${path} should not select nodes using display name`,
  );
}

function assertContains(path, expectedSnippet, message) {
  const content = read(path);
  assert.ok(content.includes(expectedSnippet), message);
}

assertUsesTag(
  'src/lib/components/tabs/NodesTab.svelte',
  'guiSelectPolicy(policyTag, node.tag)',
);
assertUsesTag(
  'src/lib/components/NodeSelector.svelte',
  'selectPolicy(\'proxy\', node.tag)',
);
assertUsesTag(
  'src/lib/components/NodeTileGrid.svelte',
  'selectPolicy(\'proxy\', node.tag)',
);
assertUsesTag(
  'src/lib/components/tabs/OverviewTab.svelte',
  'selectPolicy(groupName, tag)',
);
assertContains(
  'src/lib/components/tabs/OverviewTab.svelte',
  'disabled={nodeSwitching !== null || !isCoreRunning}',
  'OverviewTab should disable node switching when the core is not ready',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  "const isCoreAvailable = $derived(nodeScreen?.sourceStatus === 'ready');",
  'NodesTab should gate node actions on the authoritative Client Core snapshot',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'planProbeTargets({ groups, selectedGroup, visibleNodes: filteredNodes })',
  'NodesTab should plan node and url_test probes through one action',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'policyProbeTagForNode(groups, node.tag)',
  'NodesTab should route a nested url_test node card through the policy probe contract',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'isProbing={isNodeProbing(node)}',
  'NodesTab should render probe state for regular nodes and nested policy nodes',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'collectProbingPolicyNodeTags(groups, probingPolicyTags)',
  'NodesTab should expand policy probe state to every member card',
);
assertContains(
  'src/lib/components/tabs/NodesToolbar.svelte',
  'isCoreAvailable: boolean;',
  'NodesToolbar should expose core readiness instead of a full connected-state flag',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'class="node-list node-list-scroll"',
  'NodesTab should give the single-group list view a dedicated scroll container',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  `.node-list-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }`,
  'NodesTab single-group list view should fill the panel and scroll vertically',
);

console.log('nodes-select-args: ok');
