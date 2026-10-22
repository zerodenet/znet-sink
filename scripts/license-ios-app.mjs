// Collect actual complete-host resolved inputs after the first SDK build, then
// produce an asset for a second, verified build. Never mutate a built .app.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { argumentsOf, sha256, serializeInventory, verifyMaterialInputs } from './license-common.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
export function mergeIosInventories(cargo, frontend, swift) {
  if (cargo.target?.platform !== 'ios' || frontend.target?.platform !== 'frontend'
      || !['debug', 'release'].includes(cargo.target.profile)
      || !['aarch64-apple-ios', 'aarch64-apple-ios-sim', 'x86_64-apple-ios'].includes(cargo.target.triple)
      || JSON.stringify([...cargo.target.features].sort()) !== JSON.stringify([...frontend.target.features].sort())) {
    throw Error('iOS inventory target/profile/features mismatch');
  }
  if (!swift.length || swift.some(value => value.target?.platform !== 'ios'
      || value.target.triple !== cargo.target.triple || value.target.profile !== cargo.target.profile
      || value.provenance?.kind !== 'ios-application-swift-resolved-inputs')) {
    throw Error('Missing or mismatched actual application Swift inputs');
  }
  const values = [cargo, frontend, ...swift];
  for (const value of values) if (value.schemaVersion !== 1 || !value.entries?.length || !Array.isArray(value.texts) || !Array.isArray(value.gaps)) throw Error('Invalid iOS inventory');
  return {schemaVersion: 1, target: cargo.target,
    provenance: {kind: 'ios-application-resolved-input-inventory', inventories: values.map(value => value.provenance),
      inputs: values.map((value, index) => ({label: index === 0 ? 'cargo-inventory' : index === 1 ? 'frontend-inventory' : `swift-inventory-${index - 2}`, sha256: sha256(JSON.stringify(value))}))},
    entries: values.flatMap(value => value.entries), texts: values.flatMap(value => value.texts), gaps: values.flatMap(value => value.gaps)};
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = argumentsOf(process.argv.slice(2));
  for (const name of ['cargo', 'frontend', 'metadata', 'target-dir', 'sdk-path', 'output', 'evidence']) if (!args[name]) throw Error(`Missing --${name}`);
  const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
  const cargo = read(args.cargo), frontend = read(args.frontend), metadata = read(args.metadata);
  const selected = new Set(cargo.entries.filter(row => row.ecosystem === 'cargo').map(row => row.cargoPackageIdSha256));
  const packages = metadata.packages.filter(pkg => selected.has(sha256(pkg.id))
    && fs.existsSync(path.join(path.dirname(pkg.manifest_path), 'ios/Package.swift')));
  if (!packages.some(pkg => pkg.name === 'tauri-plugin-znet-native-io')) throw Error('Actual native plugin absent from iOS graph');
  const buildRoot = path.join(args['target-dir'], cargo.target.triple, cargo.target.profile, 'build');
  if (!fs.existsSync(buildRoot)) throw Error('Actual SDK build outputs absent');
  fs.mkdirSync(args.evidence, {recursive: true});
  const swift = [];
  for (const pkg of packages.sort((a, b) => a.name.localeCompare(b.name))) {
    // The build driver requires a fresh target directory, so multiple packages
    // with this identity indicate ambiguity; never select a cache by timestamp.
    const candidates = fs.readdirSync(buildRoot).filter(name => name.startsWith(`${pkg.name}-`))
      .map(name => path.join(buildRoot, name, 'out/swift-rs', pkg.name))
      .filter(directory => fs.existsSync(path.join(directory, 'workspace-state.json')));
    if (candidates.length !== 1) throw Error(`Missing or ambiguous actual Swift build: ${pkg.name}`);
    const output = path.join(args.evidence, `swift-${pkg.name}.json`);
    const lock = path.join(path.dirname(pkg.manifest_path), 'ios/Package.resolved');
    execFileSync(process.execPath, [path.join(root, 'scripts/license-swift.mjs'),
      '--cargo-metadata', args.metadata, '--host-package', pkg.name, '--application-inputs', '1',
      '--swift-build-path', candidates[0], '--target', cargo.target.triple, '--profile', cargo.target.profile,
      '--sdk-path', args['sdk-path'], '--output', output,
      ...(fs.existsSync(lock) ? ['--resolved-file', lock] : [])], {stdio: 'inherit'});
    swift.push(read(output));
  }
  const result = mergeIosInventories(cargo, frontend, swift);
  verifyMaterialInputs(result, root, ['resources/licenses/frontend-package-materials.json', 'resources/licenses/rust-package-materials.json', 'resources/licenses/swift-package-materials.json']);
  const text = serializeInventory(result);
  if (Buffer.byteLength(text) > 16 * 1024 * 1024) throw Error('iOS inventory exceeds native reader budget');
  fs.mkdirSync(path.dirname(path.resolve(args.output)), {recursive: true});
  fs.writeFileSync(args.output, text);
  console.log(`Complete iOS host input inventory: ${JSON.parse(text).entries.length} records; explicit material/evidence gaps retained.`);
}
