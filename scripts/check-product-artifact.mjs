import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { resolveProduct, tools } from './product-composition.mjs';
const product = resolveProduct(process.argv[2]);
const artifact = JSON.parse(readFileSync('build/product-composition.json', 'utf8'));
assert.equal(artifact.product, product.name);
assert.deepEqual(artifact.features, product.features);
function files(path) {
  return readdirSync(path, {withFileTypes: true}).flatMap(item => item.isDirectory() ? files(join(path, item.name)) : [join(path, item.name)]);
}
const code = files('build/_app').filter(path => path.endsWith('.js')).map(path => readFileSync(path, 'utf8')).join('\n');
const styles = files('build/_app').filter(path => path.endsWith('.css')).map(path => readFileSync(path, 'utf8')).join('\n');
for (const tool of tools) {
  const selected = product.features.includes(tool.feature);
  for (const file of tool.files) {
    assert.equal(artifact.modules.some(id => id.endsWith(`/features/${tool.directory}/${file}`)), selected, `${product.name}: ${tool.directory}/${file}`);
  }
  if (tool.style) assert.equal(styles.includes(tool.style), selected, `${product.name}: tool styles`);
  for (const command of tool.commands) assert.equal(code.includes(command), selected, `${product.name}: bundled ${command}`);
}
console.log(`${product.name}: artifact module graph and command calls verified`);
