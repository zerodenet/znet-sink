import assert from 'node:assert/strict';
import { test } from 'node:test';
import { resolveProduct, productComposition } from './product-composition.mjs';
test('product selection defaults to existing desktop and rejects misspellings', () => {
  assert.deepEqual(resolveProduct('desktop').features, ['tool-dns', 'tool-route', 'tool-node-probe']);
  assert.deepEqual(resolveProduct('desktop-base').features, []);
  assert.throws(() => resolveProduct('deskop'), /Unknown/);
});
test('single-tool products have no implicit dependency on each other', () => {
  assert.deepEqual(resolveProduct('desktop-dns').tools.map(tool => tool.id), ['dns']);
  assert.deepEqual(resolveProduct('desktop-route').tools.map(tool => tool.id), ['route-trace']);
  assert.deepEqual(resolveProduct('desktop-node-probe').tools.map(tool => tool.id), ['probe-jobs']);
});
test('trimmed tools are absent from imports and rejected if reached through another entry', () => {
  const previous = process.env.ZNET_PRODUCT;
  try {
    process.env.ZNET_PRODUCT = 'desktop-base';
    const plugin = productComposition();
    assert.equal(plugin.load('\0virtual:znet-product').includes('import '), false);
    assert.throws(() => plugin.generateBundle.call({error(message) {throw new Error(message);}}, {}, {
      leak: {type: 'chunk', modules: {'/src/lib/features/dns/client.ts': {}}},
    }), /Trimmed tool dns/);
  } finally {
    if (previous === undefined) delete process.env.ZNET_PRODUCT;
    else process.env.ZNET_PRODUCT = previous;
  }
});

test('backend tool diagnostic reader follows the selected composition even without a node page', () => {
  const previous = process.env.ZNET_PRODUCT;
  try {
    process.env.ZNET_PRODUCT = 'desktop-base';
    assert.equal(productComposition().load('\0virtual:znet-tool-diagnostics'), 'export const toolDiagnostics = [];');
    process.env.ZNET_PRODUCT = 'desktop-node-probe';
    assert.match(productComposition().load('\0virtual:znet-tool-diagnostics'), /node-probes\/diagnostics/);
    assert.match(productComposition().load('\0virtual:znet-node-probes'), /cancelProbeJob/);
  } finally {
    if (previous === undefined) delete process.env.ZNET_PRODUCT;
    else process.env.ZNET_PRODUCT = previous;
  }
});
