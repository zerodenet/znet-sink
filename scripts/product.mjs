import { spawnSync } from 'node:child_process';
import { resolveProduct } from './product-composition.mjs';
const [action, name = 'desktop', ...extra] = process.argv.slice(2);
if (!['build', 'dev', 'check'].includes(action) || extra.length) throw new Error('Usage: pnpm product <build|dev|check> <product>');
const product = resolveProduct(name);
const features = product.features.length ? ['--features', product.features.join(',')] : [];
const command = action === 'check' ? 'cargo' : 'pnpm';
const args = action === 'check'
  ? ['check', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '-p', 'gui', '--no-default-features', ...features]
  : ['exec', 'tauri', action, ...features, '--', '--no-default-features'];
const result = spawnSync(command, args, {stdio: 'inherit', env: {...process.env, ZNET_PRODUCT: name}, shell: process.platform === 'win32'});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
