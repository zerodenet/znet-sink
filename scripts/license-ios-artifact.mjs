import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { argumentsOf, sha256, finalizeInventory, verifyMaterialInputs } from './license-common.mjs';

export function iosLicenseReceipt(app, expected, frontendFile, repositoryRoot) {
  const asset = path.join(app, 'assets/license-inventory.native.json');
  const size = fs.statSync(asset).size;
  if (!size || size > 16 * 1024 * 1024) throw Error('Invalid packaged iOS license asset budget');
  const bytes = fs.readFileSync(asset);
  if (!bytes.equals(fs.readFileSync(expected))) throw Error('Packaged iOS license asset differs from collected inputs');
  const inventory = finalizeInventory(JSON.parse(bytes));
  if (inventory.target?.platform !== 'ios' || inventory.provenance?.kind !== 'ios-application-resolved-input-inventory') throw Error('Wrong iOS package inventory target/kind');
  const frontend = JSON.parse(fs.readFileSync(frontendFile));
  if (inventory.provenance.inputs.find(value => value.label === 'frontend-inventory')?.sha256 !== sha256(JSON.stringify(frontend))) throw Error('Packaged iOS inventory uses stale frontend');
  verifyMaterialInputs(inventory, repositoryRoot, ['resources/licenses/frontend-package-materials.json', 'resources/licenses/rust-package-materials.json', 'resources/licenses/swift-package-materials.json']);
  return {schemaVersion: 1, kind: 'actual-ios-application-license-asset', target: inventory.target,
    licenseAsset: {name: 'assets/license-inventory.native.json', sha256: sha256(bytes), bytes: size},
    entries: inventory.entries.length, gaps: inventory.gaps.length,
    limitation: 'The actual App contains this resolved-input inventory. Per-package final-link retention, signing, installation and device/VPN acceptance are not inferred.'};
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = argumentsOf(process.argv.slice(2));
  for (const name of ['app', 'expected', 'frontend', 'output']) if (!args[name]) throw Error(`Missing --${name}`);
  const receipt = iosLicenseReceipt(args.app, args.expected, args.frontend, fileURLToPath(new URL('../', import.meta.url)));
  fs.writeFileSync(args.output, JSON.stringify(receipt, null, 2) + '\n');
  console.log(JSON.stringify(receipt));
}
