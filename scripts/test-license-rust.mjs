import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {sha256} from './license-common.mjs';
import {rustPackageInventory} from './license-rust.mjs';
import {reviewedRegistryBinding,collectRegistryMaterials,archiveMaterial} from './license-rust-source-materials.mjs';
import {applyPackageMaterialCache} from './license-materials.mjs';

// Synthetic archive fixtures test source identity and failure boundaries. They are
// not legal materials and are never added to the checked-in original cache.
function fixture(vcs={git:{sha1:'a'.repeat(40)},path_in_vcs:'crates/fixture'}) {
 const dir=mkdtempSync(join(tmpdir(),'znet-rust-license-'));
 const source='registry+https://github.com/rust-lang/crates.io-index';
 const pkg={name:'fixture',version:'1.2.3',id:`${source}#fixture@1.2.3`,source,license:'MIT',repository:'https://github.com/example/project',targets:[{kind:['lib']}]};
 const location=join(dir,'registry/src/index/fixture-1.2.3');mkdirSync(location,{recursive:true});
 pkg.manifest_path=join(location,'Cargo.toml');writeFileSync(pkg.manifest_path,'[package]\nname="fixture"\nversion="1.2.3"\n');
 if(vcs)writeFileSync(join(location,'.cargo_vcs_info.json'),JSON.stringify(vcs));
 writeFileSync(join(location,'COPYRIGHT-RUST.txt'),'Synthetic original legal body\nCopyright Test Author\n');
 const cache=join(dir,'registry/cache/index');mkdirSync(cache,{recursive:true});
 const archive=join(cache,'fixture-1.2.3.crate');execFileSync('tar',['-czf',archive,'-C',join(dir,'registry/src/index'),'fixture-1.2.3']);
 const checksum=sha256(readFileSync(archive));
 const lock=`[[package]]\nname = "fixture"\nversion = "1.2.3"\nsource = "${source}"\nchecksum = "${checksum}"\n`;
 return {dir,pkg,lock,archive,close:()=>rmSync(dir,{recursive:true,force:true})};
}
function metadata(f) {
 const guiDir=join(f.dir,'gui');mkdirSync(guiDir);writeFileSync(join(guiDir,'Cargo.toml'),'[package]\nname="gui"\n');writeFileSync(join(guiDir,'LICENSE'),'Synthetic GUI terms\n');
 const gui={id:'path+gui',name:'gui',version:'0.0.3',manifest_path:join(guiDir,'Cargo.toml'),license:'MIT',source:null,targets:[{kind:['lib']}]};
 const dev={...f.pkg,id:'dev-only',name:'dev-only'};
 const proc={...f.pkg,id:'macro',name:'macro',targets:[{kind:['proc-macro']}]};
 const dep=id=>({pkg:id,dep_kinds:[{kind:null}]});
 return {workspace_root:f.dir,workspace_members:[gui.id],packages:[gui,f.pkg,dev,proc],resolve:{nodes:[{id:gui.id,features:[],deps:[dep(f.pkg.id),dep(proc.id),{pkg:dev.id,dep_kinds:[{kind:'dev'}]}]},{id:f.pkg.id,features:['chosen-target-feature'],deps:[]},{id:proc.id,features:[],deps:[]}]}};
}

test('registry binding proves locked archive, installed manifest and exact Cargo ID',()=>{
 const f=fixture();try{const binding=reviewedRegistryBinding(f.pkg,f.lock);assert.equal(binding.expected.registryArchiveSha256,sha256(readFileSync(f.archive)));assert.equal(binding.expected.cargoPackageIdSha256,sha256(f.pkg.id));assert.equal(binding.vcs.git.sha1,'a'.repeat(40));}finally{f.close();}
});
test('archive and installed manifest tampering fail closed',()=>{
 const f=fixture();try{assert.throws(()=>reviewedRegistryBinding(f.pkg,f.lock.replace(/checksum = "[^"]+"/,`checksum = "${'f'.repeat(64)}"`)),/checksum mismatch/);writeFileSync(f.pkg.manifest_path,'different manifest');assert.throws(()=>reviewedRegistryBinding(f.pkg,f.lock),/manifest differs/);}finally{f.close();}
});
test('network collector uses archived immutable revision, retains original bytes and exact input ties',async()=>{
 const f=fixture();try{const text='\ufeffSynthetic reviewed original terms\n';let calls=0;const result=await collectRegistryMaterials([f.pkg],f.lock,{fetchMaterials:async(base,revision)=>{calls++;assert.equal(base,'https://raw.githubusercontent.com/example/project');assert.equal(revision,'a'.repeat(40));const url=`${base}/${revision}/LICENSE`;return [{name:'LICENSE',kind:'license',source:url,text,sha256:sha256(text),origin:{url,sha256:sha256(text),revision,member:'LICENSE'}}];}});assert.equal(calls,1);assert.equal(result.packages[0].materials[0].text,text);assert.equal(result.packages[0].expected.manifestSha256,sha256(readFileSync(f.pkg.manifest_path)));assert.equal(result.packages[0].acquisition.packagePath,'crates/fixture');}finally{f.close();}
});
test('no VCS evidence remains gap, dirty VCS rejects, neither borrows latest material',async()=>{
 const f=fixture(null);try{const result=await collectRegistryMaterials([f.pkg],f.lock,{fetchMaterials:()=>{throw new Error('must not fetch');}});assert.equal(result.packages.length,0);assert.equal(result.gaps[0].reason,'published-archive-has-no-pinned-vcs-revision');}finally{f.close();}
 const dirty=fixture({git:{sha1:'a'.repeat(40),dirty:true}});try{assert.throws(()=>reviewedRegistryBinding(dirty.pkg,dirty.lock),/Dirty archived/);}finally{dirty.close();}
});
test('archive original provenance is the actual archive hash/member without extraction',()=>{
 const f=fixture();try{const b=reviewedRegistryBinding(f.pkg,f.lock);const m=archiveMaterial(f.pkg,b,'COPYRIGHT-RUST.txt','license');assert.equal(m.origin.sha256,b.expected.registryArchiveSha256);assert.equal(m.origin.member,'fixture-1.2.3/COPYRIGHT-RUST.txt');assert.equal(m.sha256,sha256(m.text));assert.throws(()=>archiveMaterial(f.pkg,b,'../other','license'),/Unsafe/);}finally{f.close();}
});
test('resolved feature graph excludes dev and labels proc-macro host tools without claiming compilation',()=>{
 const f=fixture();try{const result=rustPackageInventory(metadata(f));assert(!result.entries.some(e=>e.name==='dev-only'));const row=result.entries.find(e=>e.name==='fixture');assert.deepEqual(row.resolvedFeatures,['chosen-target-feature']);assert(row.evidence.some(e=>e.kind==='resolved-target'));assert(!row.evidence.some(e=>e.kind==='compiled'));assert(result.entries.find(e=>e.name==='macro').evidence.some(e=>e.kind==='build-tool'));assert.equal(row.registryArchiveSha256,sha256(readFileSync(f.archive)));}finally{f.close();}
});
test('offline cache applies only exact current manifest/archive and retains declaration gaps',()=>{
 const f=fixture();try{const inventory=rustPackageInventory(metadata(f));const row=inventory.entries.find(e=>e.name==='fixture');row.declaredLicense=null;row.missing.push('license-declaration-not-provided');const b=reviewedRegistryBinding(f.pkg,f.lock);const cache={schemaVersion:1,packages:[{identity:b.identity,expected:b.expected,materials:[archiveMaterial(f.pkg,b,'COPYRIGHT-RUST.txt','license')],resolves:[]}]};const result=applyPackageMaterialCache(inventory,cache);assert(result.entries.find(e=>e.name==='fixture').missing.includes('license-declaration-not-provided'));const changed=structuredClone(cache);changed.packages[0].expected.registryArchiveSha256='b'.repeat(64);assert.throws(()=>applyPackageMaterialCache(inventory,changed),/Stale/);cache.packages[0].identity.version='different';assert.equal(applyPackageMaterialCache(inventory,cache).entries.find(e=>e.name==='fixture').licenseTextIds.length,0);}finally{f.close();}
});
test('checked-in originals all validate exact text/provenance; cesu8 preserves its published complete terms',()=>{
 const cache=JSON.parse(readFileSync(new URL('../resources/licenses/rust-package-materials.json',import.meta.url),'utf8'));applyPackageMaterialCache({entries:[],texts:[],gaps:[]},cache);const cesu=cache.packages.find(p=>p.identity.name==='cesu8');assert.equal(cesu.identity.version,'1.1.0');assert.equal(cesu.materials[0].origin.member,'cesu8-1.1.0/COPYRIGHT-RUST.txt');assert(cesu.materials[0].text.includes('Apache License'));assert(cesu.materials[0].text.includes('Permission is hereby granted'));assert(!JSON.stringify(cache).includes('/workspace/'));for(const p of cache.packages){assert(/^[a-f0-9]{64}$/.test(p.expected.registryArchiveSha256));for(const m of p.materials)assert.equal(sha256(m.text),m.sha256);}
});
test('arbitrary parent legal terms are not inherited without explicit workspace license relationship',()=>{
 const f=fixture();try{const meta=metadata(f);writeFileSync(join(f.dir,'Cargo.toml'),'[workspace]\nmembers=[]\n[workspace.package]\nlicense="MIT"\n');writeFileSync(join(f.dir,'LICENSE'),'Synthetic workspace terms\n');const without=rustPackageInventory(meta).entries.find(e=>e.name==='fixture');assert.equal(without.licenseTextIds.length,0);writeFileSync(f.pkg.manifest_path,'[package]\nlicense.workspace = true\n');const withInheritance=rustPackageInventory(meta).entries.find(e=>e.name==='fixture');assert.equal(withInheritance.licenseTextIds.length,1);}finally{f.close();}
});
test('local manifest and explicitly referenced actual license are freshness inputs',async()=>{
 const {localLicenseInputs}=await import('./license-rust.mjs');const {fileURLToPath}=await import('node:url');
 const manifest=fileURLToPath(new URL('../src-tauri/crates/client-capabilities/Cargo.toml',import.meta.url));const p={id:'path+client-test',source:null,manifest_path:manifest,license_file:'../../../LICENSE'};
 const inputs=localLicenseInputs({packages:[p]},[{ecosystem:'cargo',cargoPackageIdSha256:sha256(p.id)}]);
 assert.equal(inputs.find(i=>i.label==='src-tauri/crates/client-capabilities/Cargo.toml').sha256,sha256(readFileSync(manifest)));assert.equal(inputs.find(i=>i.label==='LICENSE').sha256,sha256(readFileSync(new URL('../LICENSE',import.meta.url))));assert.deepEqual(localLicenseInputs({packages:[p]},[]),[]);
});
test('reviewed selectors common terms preserve original archive notice and missing package attribution',async()=>{
 const {applyRustMaterialCache}=await import('./license-rust.mjs');const cache=JSON.parse(readFileSync(new URL('../resources/licenses/rust-package-materials.json',import.meta.url)));const p=cache.packages.find(p=>p.identity.name==='selectors');assert(p);const notice=p.materials.find(m=>m.kind==='notice');assert.equal(Buffer.byteLength(notice.text),205);assert.equal(notice.sha256,'7523df2468fda84e4df377ee0cfd844a83742f39e0425e7c7819a82e3e25f840');assert.deepEqual(notice.origin.byteRange,{start:0,end:205});const terms=p.materials.find(m=>m.kind==='license');assert(terms.source.includes('/zerodenet/znet-sink/'));assert.equal(p.acquisition.canonicalUrl,'https://mozilla.org/MPL/2.0/');assert(p.acquisition.officialDownload.includes('403'));const row={...p.identity,...p.expected,id:'selectors',licenseTextIds:[],noticeTextIds:[],evidence:[],missing:['license-text-not-provided-by-installed-material']};const result=applyRustMaterialCache({entries:[row],texts:[],gaps:[{id:'selectors',reason:'license-text-not-provided-by-installed-material'}]},cache);assert.deepEqual(result.entries[0].missing,['package-specific-legal-material-not-provided']);assert.equal(result.entries[0].licenseTextIds.length,1);assert.equal(result.entries[0].noticeTextIds.length,1);
});
test('tao actual release comparison evidence retains all published source hashes and only Apache alternative',()=>{
 const cache=JSON.parse(readFileSync(new URL('../resources/licenses/rust-package-materials.json',import.meta.url)));const p=cache.packages.find(p=>p.identity.name==='tao-macros');assert.equal(p.acquisition.tag,'tao-macros-v0.1.3');assert.equal(p.acquisition.revision,'340684d932b6126bed449fc1f4edca81776ad592');assert.equal(p.acquisition.compared.length,6);for(const f of p.acquisition.compared)assert(/^[a-f0-9]{64}$/.test(f.sha256));assert.deepEqual(p.acquisition.excludedGenerated,['Cargo.toml','Cargo.lock']);assert.deepEqual(p.unresolved,['alternative-MIT-license-text-not-provided']);assert.equal(p.materials.length,1);assert(p.materials[0].text.includes('Version 2.0, January 2004'));assert(p.materials[0].origin.member.endsWith('/LICENSE'));
});
test('tag fallback rejects source drift even when version-specific tag name appears correct',async()=>{
 const {taoTaggedMaterials}=await import('./license-rust-source-materials.mjs');const f=fixture();try{
  const mappings=[['Cargo.toml.orig','tao-macros/Cargo.toml'],['README.md','README.md'],['CHANGELOG.md','tao-macros/CHANGELOG.md'],['src/lib.rs','tao-macros/src/lib.rs'],['examples/generate_package_name.rs','tao-macros/examples/generate_package_name.rs'],['examples/android_fn.rs','tao-macros/examples/android_fn.rs']];
  const revision='340684d932b6126bed449fc1f4edca81776ad592',tagRoot=join(f.dir,'tag',`tao-${revision}`);mkdirSync(tagRoot,{recursive:true});
  for(const [published,repo] of mappings){const source=join(f.dir,'registry/src/index/fixture-1.2.3',published),target=join(tagRoot,repo);mkdirSync(join(source,'..'),{recursive:true});mkdirSync(join(target,'..'),{recursive:true});writeFileSync(source,'synthetic corresponding source '+published);writeFileSync(target,readFileSync(source));}
  execFileSync('tar',['-czf',f.archive,'-C',join(f.dir,'registry/src/index'),'fixture-1.2.3']);
  writeFileSync(join(tagRoot,'LICENSE'),'Synthetic Apache fixture\nVersion 2.0, January 2004\n');const tag=join(f.dir,'tag.tar.gz');execFileSync('tar',['-czf',tag,'-C',join(f.dir,'tag'),`tao-${revision}`]);
  const binding={identity:{},expected:{}};assert.equal(taoTaggedMaterials(f.pkg,binding,readFileSync(tag)).acquisition.compared.length,6);
  writeFileSync(join(tagRoot,'tao-macros/src/lib.rs'),'changed upstream file');execFileSync('tar',['-czf',tag,'-C',join(f.dir,'tag'),`tao-${revision}`]);assert.throws(()=>taoTaggedMaterials(f.pkg,binding,readFileSync(tag)),/comparison failed/);
 }finally{f.close();}
});
