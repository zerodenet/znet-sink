// Receipt is external to the APK: embedding its own final checksum would be circular.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {dirname,resolve,basename} from 'node:path';
import {sha256,serializeInventory,argumentsOf,verifyMaterialInputs} from './license-common.mjs';
import {fileURLToPath} from 'node:url';
import {androidInventoryTarget} from './android-build-target.mjs';
/** @param {string} apk @param {string} expectedFrontend @param {string|null} repositoryRoot @param {string} profile */
export function apkLicenseReceipt(apk,expectedFrontend,repositoryRoot=null,profile='release') {
  if(!['debug','release'].includes(profile))throw new Error('APK inventory target/profile mismatch');
  const extracted=JSON.parse(execFileSync('python3',['-c',`
import json,sys,zipfile,hashlib
with zipfile.ZipFile(sys.argv[1]) as archive:
 name='assets/license-inventory.native.json'
 info=archive.getinfo(name)
 if info.file_size>16*1024*1024: raise ValueError('inventory exceeds budget')
 body=archive.read(name)
 payload=[]
 for item in sorted(archive.infolist(),key=lambda x:x.filename):
  if item.filename.startswith('lib/') and item.filename.endswith('.so'):
   h=hashlib.sha256()
   with archive.open(item) as f:
    for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
   payload.append({'name':item.filename,'sha256':h.hexdigest(),'bytes':item.file_size})
 print(json.dumps({'inventory':json.loads(body),'assetSha256':hashlib.sha256(body).hexdigest(),'nativePayloads':payload}))
`,apk],{encoding:'utf8',maxBuffer:20*1024*1024}));
  const inventory=extracted.inventory;
  if(inventory.schemaVersion!==1||inventory.target?.platform!=='android'||inventory.target.profile!==profile)throw new Error('APK inventory target/profile mismatch');
  const target=androidInventoryTarget(inventory.target.triple,profile);
  const frontend=JSON.parse(readFileSync(expectedFrontend,'utf8'));
  const proof=inventory.provenance.inputs?.find(/** @param {any} item */ item=>item.label==='frontend-inventory');
  if(proof?.sha256!==sha256(JSON.stringify(frontend)))throw new Error('APK embeds a stale frontend inventory');
  if(repositoryRoot)verifyMaterialInputs(inventory,repositoryRoot,['resources/licenses/frontend-package-materials.json','resources/licenses/rust-package-materials.json','resources/licenses/android-package-materials.json','resources/licenses/authoritative-materials.json']);
  if(!extracted.nativePayloads.some(/** @param {any} item */ item=>item.name===`lib/${target.abi}/libgui_lib.so`) ||
    extracted.nativePayloads.some(/** @param {any} item */ item=>item.name.split('/')[1]!==target.abi))throw new Error('Android Rust host native payload missing or ABI mismatch');
  const receipt={schemaVersion:1,kind:profile==='debug'?'actual-debug-android-package-license-receipt':'actual-unsigned-android-package-receipt',artifact:{name:basename(apk),sha256:sha256(readFileSync(apk))},licenseAsset:{name:'assets/license-inventory.native.json',sha256:extracted.assetSha256},nativePayloads:extracted.nativePayloads,
    limitation:'APK inclusion proves the inventory and native payload exist. This license receipt does not verify signing or installability. Cargo target resolve and Gradle runtimeClasspath entries remain resolved inputs; no package-by-package linker/R8 retention is inferred.'};
  inventory.provenance={...inventory.provenance,packageReceipt:receipt};
  return {inventory,receipt};
}
if(process.argv[1]===fileURLToPath(import.meta.url)) {
 const args=argumentsOf(process.argv.slice(2));if(!args.apk||!args.frontend||!args.output)throw new Error('--apk --frontend --output required');
 const {inventory,receipt}=apkLicenseReceipt(args.apk,args.frontend,fileURLToPath(new URL('../',import.meta.url)),args.profile||'release');mkdirSync(dirname(resolve(args.output)),{recursive:true});writeFileSync(args.output,serializeInventory(inventory));
 if(args.receipt)writeFileSync(args.receipt,JSON.stringify(receipt,null,2)+'\n');
 console.log(`Actual package license receipt: ${inventory.entries.length} records, ${inventory.gaps.length} explicit gaps; no final-link retention inference`);
}
