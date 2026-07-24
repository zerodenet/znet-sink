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

console.log('desktop-shell: ok');
