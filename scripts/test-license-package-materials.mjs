import assert from 'node:assert/strict';
import {test} from 'node:test';
import {applyPackageMaterialCache} from './license-materials.mjs';
import {sha256,finalizeInventory,verifyMaterialInputs} from './license-common.mjs';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

const row={id:'cargo:fixture',ecosystem:'cargo',name:'fixture',version:'1.2.3',source:'registry+https://example.org/index',manifestSha256:sha256('manifest'),licenseTextIds:[],noticeTextIds:[],materialReferences:[],evidence:[],missing:['license-body-missing','package-specific-legal-material-not-provided','license-declaration-not-provided'],textStatus:'missing'};
const inventory={entries:[row],texts:[],gaps:row.missing.map(reason=>({id:row.id,reason}))};
const material={name:'LICENSE',kind:'license',source:'https://example.org/repo/exact/LICENSE',text:'Original fixture license\nCopyright Fixture Author\n',origin:{url:'https://example.org/archive/exact.tar.gz',sha256:sha256('archive'),member:'LICENSE',revision:'exact'}};
material.sha256=sha256(material.text);
const pkg={identity:Object.fromEntries(['ecosystem','name','version','source'].map(key=>[key,row[key]])),expected:{manifestSha256:row.manifestSha256},materials:[material],resolves:[]};
const cache={schemaVersion:1,packages:[pkg]};
test('exact package originals preserve copyright, provenance and independent declaration gaps',()=>{
 const result=finalizeInventory(applyPackageMaterialCache(inventory,cache));
 assert.equal(result.entries[0].textStatus,'available');
 assert.deepEqual(result.entries[0].missing,['package-specific-legal-material-not-provided','license-declaration-not-provided']);
 assert.deepEqual(result.entries[0].materialReferences[0].origin,material.origin);
 assert.equal(result.texts[0].text,material.text);
 assert(!result.gaps.some(g=>g.reason==='license-body-missing'));
 assert.equal(inventory.entries[0].licenseTextIds.length,0,'input is not mutated');
});
for(const field of ['ecosystem','name','version','source'])test(`different ${field} never borrows another package license`,()=>{
 const changed=structuredClone(cache);changed.packages[0].identity[field]+='-different';
 assert.equal(applyPackageMaterialCache(inventory,changed).texts.length,0);
});
test('identity with stale source input fails instead of silently borrowing',()=>{
 const changed=structuredClone(cache);changed.packages[0].expected.manifestSha256=sha256('changed');
 assert.throws(()=>applyPackageMaterialCache(inventory,changed),/Stale package/);
});
test('notice does not become full license or resolve source completeness without review',()=>{
 const changed=structuredClone(cache);changed.packages[0].materials[0].kind='notice';
 const result=applyPackageMaterialCache(inventory,changed);
 assert.equal(result.entries[0].textStatus,'missing');assert.equal(result.entries[0].noticeTextIds.length,1);
 assert.deepEqual(result.entries[0].missing,row.missing);
});
test('reviewed package completeness clears only its own scope',()=>{
 const changed=structuredClone(cache);changed.packages[0].resolves=['package-specific-legal-material-not-provided'];
 const result=applyPackageMaterialCache(inventory,changed);
 assert.deepEqual(result.entries[0].missing,['license-declaration-not-provided']);
 assert.deepEqual(result.gaps,[{id:row.id,reason:'license-declaration-not-provided'}]);
});
for(const [label,change,pattern] of [
 ['tampered original',c=>{c.packages[0].materials[0].text+='tampered';},/digest/],
 ['missing original origin',c=>{delete c.packages[0].materials[0].origin;},/provenance/],
 ['unbound identifier only',c=>{c.packages[0].expected={license:'MIT'};},/input hashes/],
 ['unrelated gap removal',c=>{c.packages[0].resolves=['license-declaration-not-provided'];},/unrelated/],
 ['duplicate identity',c=>{c.packages.push(structuredClone(c.packages[0]));},/Duplicate/],
])test(label,()=>{const changed=structuredClone(cache);change(changed);assert.throws(()=>applyPackageMaterialCache(inventory,changed),pattern);});
test('packaging checks nested cache input hashes and missing provenance',()=>{
 const root=mkdtempSync(join(tmpdir(),'znet-material-input-'));
 try{
  writeFileSync(join(root,'cache.json'),'reviewed-originals');
  const composed={provenance:{inventories:[{inputs:[{label:'cache.json',sha256:sha256('reviewed-originals')}]}]}};
  verifyMaterialInputs(composed,root,['cache.json']);
  assert.throws(()=>verifyMaterialInputs({provenance:{}},root,['cache.json']),/Stale or missing/);
  writeFileSync(join(root,'cache.json'),'changed-originals');
  assert.throws(()=>verifyMaterialInputs(composed,root,['cache.json']),/Stale or missing/);
 }finally{rmSync(root,{recursive:true,force:true});}
});
