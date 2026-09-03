import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse, compile } from 'svelte/compiler';

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)]);
}
function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  visit(node);
  for (const value of Object.values(node)) {
    if (Array.isArray(value)) value.forEach((child) => walk(child, visit));
    else if (value && typeof value === 'object') walk(value, visit);
  }
}
const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
let scanned = 0;
for (const file of files(fileURLToPath(new URL('../src', import.meta.url))).filter((file) => file.endsWith('.svelte'))) {
  const source = readFileSync(file, 'utf8').replace(/^\uFEFF/, '');
  compile(source, { filename: file, generate: 'client' });
  if (file.replaceAll('\\', '/').includes('/components/ui/')) continue;
  scanned++;
  walk(parse(source).html, (node) => {
    if (node.type !== 'Element') return;
    assert.ok(!['select', 'input', 'textarea'].includes(node.name), `${file}: use shared form controls, not <${node.name}>`);
    if (node.name !== 'button') return;
    const attribute = (name) => node.attributes.find((a) => a.name === name)?.value?.[0]?.data;
    assert.equal(attribute('data-slot'), 'surface-button', `${file}: ordinary actions must use Button; only layout-specific hit targets may use surface-button`);
    assert.ok(!['tab', 'radio'].includes(attribute('role')), `${file}: use AppTabs or AppSegmentedControl`);
  });
}
const fields = read('src/lib/components/ui/controls.css');
assert.ok(fields.includes('-webkit-appearance: none') && fields.includes('appearance: textfield'));
assert.ok(fields.includes('.znet-choice[type=\'radio\']:checked') && fields.includes('forced-colors: active'));
for (const file of ['input/input.svelte', 'textarea/textarea.svelte', 'select/select-trigger.svelte']) {
  assert.ok(read(`src/lib/components/ui/${file}`).includes('znet-field'), `${file}: missing shared field surface`);
}
const menu = read('src/lib/components/ui/select/select-content.svelte');
assert.ok(menu.includes('SelectPortal') && menu.includes('--layer-menu') && menu.includes('--bits-floating-available-height'));
assert.ok(menu.includes('--bits-floating-anchor-width') && menu.includes('min-h-0') && !menu.includes('--bits-select-'), 'menu sizing must use current Bits floating variables and a shrinkable scrolling viewport');
for (const name of ['DraggableModal', 'ActionConfirmDialog', 'ConnectionDetailsDrawer']) {
  assert.ok(read(`src/lib/components/${name}.svelte`).includes('if (isNestedOverlayEvent(event)) return;'), `${name}: do not consume menu Escape/Tab`);
}
assert.ok(read('src/lib/components/tabs/RulesTab.svelte').includes('Number(value)'), 'update intervals must remain numeric across the string-valued selector');
console.log(`UI control contract passed (${scanned} consumer components scanned).`);
