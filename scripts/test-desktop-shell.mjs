import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { isRefreshShortcut } from '../src/lib/services/desktop-webview.ts';
import { getTabTransitionDirection } from '../src/lib/utils/tab-transition.ts';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
}

assert.equal(isRefreshShortcut({ key: 'F5', ctrlKey: false, metaKey: false }), true);
assert.equal(isRefreshShortcut({ key: 'r', ctrlKey: true, metaKey: false }), true);
assert.equal(isRefreshShortcut({ key: 'R', ctrlKey: false, metaKey: true }), true);
assert.equal(isRefreshShortcut({ key: 'r', ctrlKey: false, metaKey: false }), false);
assert.equal(isRefreshShortcut({ key: 'f', ctrlKey: true, metaKey: false }), false);

const originalDocument = globalThis.document;
const originalWindow = globalThis.window;
const fakeDocument = new EventTarget();
const fakeWindow = new EventTarget();
globalThis.document = fakeDocument;
globalThis.window = fakeWindow;

const { installDesktopWebviewGuards } = await import('../src/lib/services/desktop-webview.ts');
const uninstallGuards = installDesktopWebviewGuards(false);

const contextMenuEvent = new Event('contextmenu', { cancelable: true });
assert.equal(fakeDocument.dispatchEvent(contextMenuEvent), false);
assert.equal(contextMenuEvent.defaultPrevented, true);

const refreshEvent = new Event('keydown', { cancelable: true });
Object.defineProperties(refreshEvent, {
  key: { value: 'r' },
  ctrlKey: { value: true },
  metaKey: { value: false },
});
assert.equal(fakeWindow.dispatchEvent(refreshEvent), false);
assert.equal(refreshEvent.defaultPrevented, true);

uninstallGuards();
globalThis.document = originalDocument;
globalThis.window = originalWindow;

const tabOrder = ['overview', 'nodes', 'profiles', 'settings', 'debug'];
assert.equal(getTabTransitionDirection(tabOrder, 'overview', 'profiles'), 1);
assert.equal(getTabTransitionDirection(tabOrder, 'settings', 'nodes'), -1);
assert.equal(getTabTransitionDirection(tabOrder, 'unknown', 'nodes'), 1);

const page = read('src/routes/+page.svelte');
assert.ok(
  page.includes('installDesktopWebviewGuards()'),
  'the root page should install production WebView guards',
);
assert.ok(
  page.includes('in:fly=') && page.includes('out:fly='),
  'top-level pages should use directional horizontal transitions',
);
assert.ok(
  page.includes(':global(.animate-fade-in)'),
  'the transition viewport should suppress page-level vertical entrance animations',
);

const baseTabsList = read('src/lib/components/ui/tabs/tabs-list.svelte');
const baseTabsTrigger = read('src/lib/components/ui/tabs/tabs-trigger.svelte');
const appTabsList = read('src/lib/components/AppTabs/List.svelte');
const appTabsTrigger = read('src/lib/components/AppTabs/Trigger.svelte');
const segmentedRoot = read('src/lib/components/AppSegmentedControl/Root.svelte');
const segmentedItem = read('src/lib/components/AppSegmentedControl/Item.svelte');
const appHeader = read('src/lib/components/AppHeader.svelte');
const debugTab = read('src/lib/components/tabs/DebugTab.svelte');
const coreConfigPanel = read('src/lib/components/settings/CoreConfigPanel.svelte');
const nodesGridCard = read('src/lib/components/tabs/NodesGridCard.svelte');
const segmentedConsumers = [
  'src/lib/components/TitleBar.svelte',
  'src/lib/components/settings/AppConfigPanel.svelte',
  'src/lib/components/tabs/OverviewTab.svelte',
  'src/lib/components/tabs/NodesToolbar.svelte',
  'src/lib/components/tabs/SubscriptionsTab.svelte',
  'src/lib/components/tabs/RulesTab.svelte',
  'src/lib/components/tabs/ProfilesTab.svelte',
  'src/lib/components/core/LogPanel.svelte',
].map(read);
assert.ok(
  !baseTabsList.includes('var(--segment-') && !baseTabsTrigger.includes('var(--segment-'),
  'upstream shadcn Tabs resources should remain unmodified by project styling',
);
assert.ok(
  appTabsList.includes('var(--segment-active-bg)') &&
    appTabsList.includes('var(--segment-active-shadow)') &&
    appTabsList.includes("[data-state='active']") &&
    appTabsTrigger.includes('BaseTrigger'),
  'AppTabs should compose the upstream primitives and match the bits-ui selected state',
);
assert.ok(
  appHeader.includes("from '$lib/components/AppTabs'") &&
    debugTab.includes("from '$lib/components/AppTabs'") &&
    !debugTab.includes(".debug-subtab[aria-pressed='true']"),
  'top navigation and debug subtabs should consume AppTabs without local selected-state overrides',
);
assert.ok(
  coreConfigPanel.includes("from '$lib/components/AppTabs'") &&
    coreConfigPanel.includes('<Tabs.Trigger') &&
    !coreConfigPanel.includes('class:active={activeChannel === ch}'),
  'kernel release channels should consume AppTabs without a local selected-state override',
);
assert.ok(
  segmentedRoot.includes('<ToggleGroup.Root') &&
    segmentedRoot.includes('onValueChange={handleValueChange}') &&
    segmentedItem.includes('<ToggleGroup.Item') &&
    segmentedItem.includes("[data-state='on']") &&
    segmentedItem.includes('var(--segment-active-bg)') &&
    segmentedItem.includes('var(--segment-active-shadow)'),
  'AppSegmentedControl should compose bits-ui and own the required single-selection surface',
);
assert.ok(
  segmentedConsumers.every((source) =>
    source.includes("from '$lib/components/AppSegmentedControl'"),
  ),
  'all compact exclusive selectors should consume AppSegmentedControl',
);
assert.ok(
  segmentedConsumers.every(
    (source) =>
      !source.includes('segment-item') &&
      !source.includes('view-switch-button') &&
      !source.includes('proxy-seg-btn') &&
      !source.includes('source-button') &&
      !source.includes('source-btn') &&
      !source.includes('filter-button'),
  ),
  'segmented control consumers should not retain page-level selected-state implementations',
);
assert.ok(
  !/class="grid-card-wrap"[\s\S]{0,100}onmouseenter/.test(nodesGridCard) &&
    /class="grid-history-trigger"[\s\S]{0,160}onmouseenter/.test(nodesGridCard),
  'grid delay history should only be triggered from the bottom status region',
);
assert.ok(
  nodesGridCard.includes('<RefreshCw />') &&
    nodesGridCard.includes('size="icon-xs"') &&
    nodesGridCard.includes('variant="ghost"') &&
    nodesGridCard.includes('onmouseenter={() => onHidePopover(0)}') &&
    !nodesGridCard.includes('{:else}测速{/if}'),
  'the grid probe action should be an unobstructed icon button',
);
assert.ok(
  /class="grid-card-header"[\s\S]{0,180}<Check \/>[\s\S]{0,180}grid-card-name/.test(
    nodesGridCard,
  ) && nodesGridCard.includes('padding-right: 20px'),
  'the selected marker should sit before the node name and reserve the probe action space',
);

console.log('desktop-shell: ok');
