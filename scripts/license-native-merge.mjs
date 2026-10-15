import { readFileSync,writeFileSync,mkdirSync,existsSync } from 'node:fs';
import { dirname,resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256,serializeInventory,argumentsOf,verifyMaterialInputs } from './license-common.mjs';
import {applyMaterialCache,applyPackageMaterialCache} from './license-materials.mjs';

/** @param {any} gradle @param {any} cargo @param {any} frontend @param {any[]} compilerMessages */
export function mergeInventories(gradle,cargo,frontend,compilerMessages=[]) {
  for(const input of [gradle,cargo,frontend])if(input.schemaVersion!==1||!Array.isArray(input.entries)||!Array.isArray(input.texts))throw new Error('Invalid inventory schema');
  if(gradle.target.platform!=='android'||cargo.target.platform!=='android'||cargo.target.profile!=='release'||frontend.target.platform!=='frontend')throw new Error('Mismatched native inventory target');
  if(JSON.stringify([...cargo.target.features].sort())!==JSON.stringify([...frontend.target.features].sort()))throw new Error('Frontend/native inventory feature mismatch');
  const rows=[...cargo.entries,...frontend.entries,...gradle.entries].map(/** @param {any} entry */ entry=>structuredClone(entry));
  for(const message of compilerMessages) {
    if(message.reason!=='compiler-artifact'||!message.package_id||!message.filenames?.length)continue;
    const generated=message.filenames.filter(/** @param {string} path */ path=>/\.(rlib|so|a|dll|dylib)$/.test(path));
    if(!generated.length||generated.some(/** @param {string} path */ path=>!existsSync(path)))continue;
    const row=rows.find(/** @param {any} row */ row=>row.ecosystem==='cargo'&&row.cargoPackageIdSha256===sha256(message.package_id));
    if(!row)continue;
    row.evidence.push({kind:'compiled',detail:'Compiler receipt observes code-generated outputs. Final linker/R8 retention is not inferred.',
      outputSha256:generated.map(/** @param {string} path */ path=>sha256(readFileSync(path))).sort(),targetKinds:message.target.kind,features:message.features});
  }
  return {schemaVersion:1,target:cargo.target,provenance:{kind:'android-packaged-license-inventory',
    inventories:[gradle.provenance,cargo.provenance,frontend.provenance],
    inputs:[{label:'gradle-inventory',sha256:sha256(JSON.stringify(gradle))},{label:'cargo-inventory',sha256:sha256(JSON.stringify(cargo))},{label:'frontend-inventory',sha256:sha256(JSON.stringify(frontend))}]},
    entries:rows,texts:[...cargo.texts,...frontend.texts,...gradle.texts],gaps:[...cargo.gaps,...frontend.gaps,...gradle.gaps]};
}
if(process.argv[1]===fileURLToPath(import.meta.url)) {
  const args=argumentsOf(process.argv.slice(2));
  const gradle=args.gradle,cargo=args.cargo||process.env.ZNET_LICENSE_INVENTORY,frontend=args.frontend||process.env.ZNET_LICENSE_FRONTEND;
  if(!gradle||!cargo||!frontend||!args.output)throw new Error('--gradle, --cargo, --frontend and --output are required');
  const read=/** @param {string} path */ path=>JSON.parse(readFileSync(path,'utf8'));
  const messages=args['cargo-messages']?readFileSync(args['cargo-messages'],'utf8').split('\n').flatMap(line=>{try{return [JSON.parse(line)];}catch{return [];}}):[];
  let inventory=mergeInventories(read(gradle),read(cargo),read(frontend),messages);
  verifyMaterialInputs(inventory,fileURLToPath(new URL('../',import.meta.url)),['resources/licenses/frontend-package-materials.json','resources/licenses/rust-package-materials.json']);
  if(args['materials-cache']){inventory=applyMaterialCache(inventory,read(args['materials-cache']));inventory.provenance.inputs.push({label:'resources/licenses/authoritative-materials.json',sha256:sha256(readFileSync(args['materials-cache']))});}
  const packageLabel='resources/licenses/android-package-materials.json';
  const packagePath=fileURLToPath(new URL(`../${packageLabel}`,import.meta.url));
  if(existsSync(packagePath)){
    inventory=applyPackageMaterialCache(inventory,read(packagePath));
    inventory.provenance.inputs.push({label:packageLabel,sha256:sha256(readFileSync(packagePath))});
  }
  mkdirSync(dirname(resolve(args.output)),{recursive:true});writeFileSync(args.output,serializeInventory(inventory));
  console.log(`Native packaged inventory: ${inventory.entries.length} records, ${inventory.gaps.length} explicit gaps`);
}
