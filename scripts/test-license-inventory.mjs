import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {apkLicenseReceipt} from './license-artifact.mjs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {sha256,readTextMaterials,attachMaterials,serializeInventory,safeSource} from './license-common.mjs';
import {mergeInventories} from './license-native-merge.mjs';
import {applyMaterialCache,declaredLicenseUrls} from './license-materials.mjs';
import {npmOwner} from './license-vite.mjs';
const dir=mkdtempSync(join(tmpdir(),'znet-license-tests-'));
try {
  writeFileSync(join(dir,'LICENSE.spdx'),'SPDXVersion: SPDX-2.1\nPackageLicenseDeclared: MIT');
  writeFileSync(join(dir,'NOTICE'),'Copyright actual author');
  const meta=readTextMaterials(dir,'fixture');
  assert.equal(meta.texts.filter(t=>t.kind==='license').length,0,'SPDX declaration is not a full license');
  assert.equal(attachMaterials({declaredLicense:'MIT'},meta).textStatus,'missing');
  writeFileSync(join(dir,'LICENSE'),'Full actual terms');
  writeFileSync(join(dir,'NOTICE.bom'),Buffer.from('\uFEFFOriginal UTF-8 notice\n'));
  writeFileSync(join(dir,'COPYING.bad'),Buffer.from([255,254]));
  mkdirSync(join(dir,'node_modules'));writeFileSync(join(dir,'node_modules','LICENSE'),'unrelated');
  const material=readTextMaterials(dir,'fixture');
  assert(material.gaps.some(g=>g.reason==='license-material-not-utf8'));
  assert(!material.texts.some(t=>t.text==='unrelated'));
  const bom=material.texts.find(t=>t.name==='NOTICE.bom');
  assert.equal(sha256(Buffer.from(bom.text,'utf8')),bom.sha256,'UTF-8 BOM bytes are preserved');
  const row=attachMaterials({id:'row',ecosystem:'npm',name:'x',declaredLicense:'MIT',evidence:[]},material);
  const inventory={schemaVersion:1,target:{platform:'frontend',features:[]},entries:[row],texts:material.texts,gaps:[]};
  assert.equal(serializeInventory(inventory),serializeInventory(inventory),'deterministic serialization');
  assert.throws(()=>serializeInventory({...inventory,texts:material.texts.map(t=>({...t,text:'tampered'}))}),/digest/);
  assert.throws(()=>serializeInventory({...inventory,entries:[{...row,licenseTextIds:['absent']}]}),/referenced/);
  assert(!safeSource('https://user:token@example.com/path?secret=private&rev=123').includes('token'));
  assert(!safeSource('https://user:token@example.com/path?secret=private&rev=123').includes('private'));
  assert.equal(npmOwner(join(dir,'LICENSE')),null,'local source is not an npm bundle owner');
  const cache={materials:[{source:'actual-installed-source',sha256:sha256('Common terms'),text:'Common terms',declaredUrls:['https://www.apache.org/licenses/LICENSE-2.0.txt']}]};
  const missing={...row,id:'missing',licenseTextIds:[],noticeTextIds:[],textStatus:'missing',missing:['license-body-missing'],evidence:[{kind:'build-input',detail:'POM license URL: https://www.apache.org/licenses/LICENSE-2.0.txt'}]};
  assert.equal(declaredLicenseUrls(missing).length,1);
  const filled=applyMaterialCache({...inventory,entries:[missing],gaps:[{id:'missing',reason:'license-body-missing'}]},cache);
  assert.equal(filled.entries[0].textStatus,'available');
  assert(filled.entries[0].missing.includes('package-specific-legal-material-not-provided'),'common terms do not create component notices');
  const unbound=applyMaterialCache({...inventory,entries:[{...missing,evidence:[]}]},cache);
  assert.equal(unbound.entries[0].licenseTextIds.length,0,'no identifier based guessing');
  assert.throws(()=>applyMaterialCache({...inventory,entries:[missing]},{materials:[{...cache.materials[0],text:'tampered'}]}),/digest/);
  const android={...inventory,target:{platform:'android',triple:'aarch64-linux-android',profile:'release',features:[]},provenance:{kind:'fixture'},entries:[],texts:[]};
  assert.throws(()=>mergeInventories(android,{...android,target:{...android.target,platform:'ios'}},{...inventory,provenance:{kind:'fixture'}}),/target/);
  assert.throws(()=>mergeInventories(android,android,{...inventory,target:{...inventory.target,features:['different']},provenance:{kind:'fixture'}}),/feature/);
  const frontend={...inventory,provenance:{kind:'fixture'}};
  const debug={...android,target:{...android.target,profile:'debug'}};
  assert.equal(mergeInventories(android,android,frontend).target.profile,'release');
  assert.equal(mergeInventories(debug,debug,frontend,[],'debug').target.profile,'debug');
  const x86={...debug,target:{...debug.target,triple:'i686-linux-android'}};
  assert.equal(mergeInventories(x86,x86,frontend,[],'debug').target.triple,'i686-linux-android');
  assert.throws(()=>mergeInventories(debug,x86,frontend,[],'debug'),/target\/profile/);
  assert.throws(()=>mergeInventories(x86,debug,frontend,[],'debug'),/target\/profile/);
  const x86Release={...x86,target:{...x86.target,profile:'release'}};
  assert.throws(()=>mergeInventories(x86Release,x86Release,frontend),/target\/profile/,'x86 Release is not adapted');
  assert.throws(()=>mergeInventories(debug,debug,frontend),/target\/profile/,'default Release must reject Debug');
  for(const [gradle,cargo,profile] of [
    [android,debug,'debug'],[debug,android,'debug'],[debug,debug,'release'],
    [android,android,'debug'],[debug,debug,'staging'],
    [{...debug,target:{...debug.target,platform:'ios'}},debug,'debug'],
    [{...debug,target:{...debug.target,triple:'x86_64-linux-android'}},debug,'debug'],
    [debug,{...debug,target:{...debug.target,triple:'x86_64-linux-android'}},'debug'],
    [{...debug,target:{...debug.target,triple:undefined}},debug,'debug'],
  ])assert.throws(()=>mergeInventories(gradle,cargo,frontend,[],profile),/target\/profile/);
  const staging={...android,target:{...android.target,profile:'staging'}};
  assert.throws(()=>mergeInventories(staging,staging,frontend,[],'staging'),/target\/profile/);
  const frontendPath=join(dir,'frontend.json'),apk=join(dir,'fixture.apk');
  writeFileSync(frontendPath,JSON.stringify(inventory));
  const packaged={...android,target:{...android.target,triple:'aarch64-linux-android'},provenance:{kind:'fixture',inputs:[{label:'frontend-inventory',sha256:sha256(JSON.stringify(inventory))}]},gaps:[]};
  execFileSync('python3',['-c',"import zipfile,sys; a=zipfile.ZipFile(sys.argv[1],'w'); a.writestr('assets/license-inventory.native.json',sys.argv[2]); a.writestr('lib/arm64-v8a/libgui_lib.so',b'synthetic test payload'); a.close()",apk,JSON.stringify(packaged)]);
  assert.equal(apkLicenseReceipt(apk,frontendPath).receipt.nativePayloads.length,1);
  const x86Packaged={...packaged,target:{...x86.target}};
  const packX86=(value,path='lib/x86/libgui_lib.so',extra=false)=>execFileSync('python3',['-c',"import zipfile,sys; a=zipfile.ZipFile(sys.argv[1],'w'); a.writestr('assets/license-inventory.native.json',sys.argv[2]); a.writestr(sys.argv[3],b'synthetic test payload'); a.writestr('lib/arm64-v8a/extra.so',b'synthetic') if sys.argv[4]=='true' else None; a.close()",apk,JSON.stringify(value),path,String(extra)]);
  packX86(x86Packaged);
  assert.equal(apkLicenseReceipt(apk,frontendPath,null,'debug').inventory.target.triple,'i686-linux-android');
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath),/target\/profile/,'x86 Debug is not Release');
  packX86(x86Packaged,'lib/arm64-v8a/libgui_lib.so');
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath,null,'debug'),/payload|ABI/);
  packX86(x86Packaged,'lib/x86/libgui_lib.so',true);
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath,null,'debug'),/payload|ABI/);
  execFileSync('python3',['-c',"import zipfile,sys; a=zipfile.ZipFile(sys.argv[1],'w'); a.writestr('assets/license-inventory.native.json',sys.argv[2]); a.writestr('lib/arm64-v8a/libgui_lib.so',b'synthetic test payload'); a.close()",apk,JSON.stringify(packaged)]);
  writeFileSync(frontendPath,JSON.stringify({...inventory,provenance:{kind:'stale'}}));
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath),/stale frontend/);

  // Exercise the actual CLI merge and APK receipt paths using synthetic ZIP
  // fixtures only. Real reviewed cache hashes are freshness inputs, not legal
  // attribution inferred for these fixture packages.
  const labels=['resources/licenses/frontend-package-materials.json','resources/licenses/rust-package-materials.json'];
  const cliFrontend={...frontend,provenance:{kind:'fixture',inputs:labels.map(label=>({label,sha256:sha256(readFileSync(new URL(`../${label}`,import.meta.url)))}))}};
  writeFileSync(frontendPath,JSON.stringify(cliFrontend));
  const gradlePath=join(dir,'gradle.json'),cargoPath=join(dir,'cargo.json'),mergedPath=join(dir,'native.json');
  writeFileSync(gradlePath,JSON.stringify(debug));writeFileSync(cargoPath,JSON.stringify(debug));
  const mergeArgs=[fileURLToPath(new URL('./license-native-merge.mjs',import.meta.url)),'--gradle',gradlePath,'--cargo',cargoPath,'--frontend',frontendPath,'--output',mergedPath,'--materials-cache',fileURLToPath(new URL('../resources/licenses/authoritative-materials.json',import.meta.url))];
  const run=args=>execFileSync(process.execPath,args,{encoding:'utf8',stdio:'pipe'});
  assert.throws(()=>run(mergeArgs),/target\/profile/);
  run([...mergeArgs,'--profile','debug']);
  const merged=JSON.parse(readFileSync(mergedPath,'utf8'));
  assert.equal(merged.target.profile,'debug');
  assert.equal(merged.target.triple,'aarch64-linux-android');
  assert.throws(()=>run([...mergeArgs,'--profile','staging']),/target\/profile/);
  const pack=value=>execFileSync('python3',['-c',"import zipfile,sys; a=zipfile.ZipFile(sys.argv[1],'w'); a.writestr('assets/license-inventory.native.json',sys.argv[2]); a.writestr('lib/arm64-v8a/libgui_lib.so',b'synthetic test payload'); a.close()",apk,JSON.stringify(value)]);
  pack(merged);
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath),/target\/profile/);
  const receipt=apkLicenseReceipt(apk,frontendPath,null,'debug').receipt;
  assert.equal(receipt.kind,'actual-debug-android-package-license-receipt');
  assert.equal(receipt.artifact.sha256,sha256(readFileSync(apk)));
  assert(receipt.limitation.includes('does not verify signing or installability'));
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath,null,'staging'),/target\/profile/);
  const receiptPath=join(dir,'receipt.json'),withReceipt=join(dir,'with-receipt.json');
  const artifactArgs=[fileURLToPath(new URL('./license-artifact.mjs',import.meta.url)),'--apk',apk,'--frontend',frontendPath,'--output',withReceipt,'--receipt',receiptPath];
  assert.throws(()=>run(artifactArgs),/target\/profile/);
  run([...artifactArgs,'--profile','debug']);
  assert.equal(JSON.parse(readFileSync(receiptPath,'utf8')).kind,'actual-debug-android-package-license-receipt');
  for(const target of [{...merged.target,profile:'release'},{...merged.target,profile:'staging'},{...merged.target,triple:'x86_64-linux-android'},{...merged.target,platform:'ios'}]) {
    pack({...merged,target});
    assert.throws(()=>apkLicenseReceipt(apk,frontendPath,null,'debug'),/target\/profile/);
  }
  pack(merged);
  writeFileSync(frontendPath,JSON.stringify({...cliFrontend,provenance:{kind:'stale-debug'}}));
  assert.throws(()=>apkLicenseReceipt(apk,frontendPath,null,'debug'),/stale frontend/);
  writeFileSync(frontendPath,JSON.stringify(cliFrontend));
  pack({...merged,provenance:{...merged.provenance,inputs:merged.provenance.inputs.map(input=>input.label==='resources/licenses/android-package-materials.json'?{...input,sha256:'0'.repeat(64)}:input)}});
  assert.throws(()=>run([...artifactArgs,'--profile','debug']),/Stale or missing package material input/);
  writeFileSync(gradlePath,JSON.stringify(android));writeFileSync(cargoPath,JSON.stringify(android));
  run(mergeArgs);
  const releaseMerged=JSON.parse(readFileSync(mergedPath,'utf8'));
  assert.equal(releaseMerged.target.profile,'release');
  pack(releaseMerged);
  run(artifactArgs);
  assert.equal(JSON.parse(readFileSync(receiptPath,'utf8')).kind,'actual-unsigned-android-package-receipt');
  console.log('License inventory tests: provenance/material/target/artifact assertions passed (including exact UTF-8 BOM bytes and explicit Debug/Release isolation)');
}finally{rmSync(dir,{recursive:true,force:true});}
