import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
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
  'selectPolicy(policyTag, node.tag)',
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
  'const isCoreAvailable = $derived(guiState.isProcessRunning);',
  'NodesTab should gate node actions on core readiness, not full proxy-connected state',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'await probeController.handleProbeAll(filteredNodes);',
  'NodesTab should batch probe the current filtered node set',
);
assertContains(
  'src/lib/components/tabs/NodesTab.svelte',
  'isProbing={probingNodeIds.has(node.id)}',
  'NodesTab should render probe state per visible node instance',
);
assertContains(
  'src/lib/components/tabs/NodesToolbar.svelte',
  'isCoreAvailable: boolean;',
  'NodesToolbar should expose core readiness instead of a full connected-state flag',
);

console.log('nodes-select-args: ok');
