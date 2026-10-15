import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {ABOUT_LINKS,ABOUT_IDENTITY,MobileAboutController,unavailableKernelVersions} from '../src/lib/features/mobile/about.ts';
import {mobileAboutGateway} from '../src/lib/services/mobile-about.ts';
const base=()=>({name:async()=> 'Actual Sink',version:async()=> '0.0.3-real',identifier:async()=>ABOUT_IDENTITY.declaredIdentifier,open:async()=>{},copy:async()=>{}});

test('actual Tauri app methods read host metadata; constructing/loading never opens browser or sends feedback',async()=>{
 const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return {'plugin:app|name':'Actual Sink','plugin:app|version':'0.0.3-host','plugin:app|identifier':ABOUT_IDENTITY.declaredIdentifier}[command];}}};
 try{const c=new MobileAboutController(mobileAboutGateway,()=>{});assert.deepEqual(calls,[]);await c.load();
 assert.equal(c.view.version,'0.0.3-host');assert.equal(c.view.name,'Actual Sink');assert.equal(c.view.identifier,ABOUT_IDENTITY.declaredIdentifier);
 assert.deepEqual(calls.map(c=>c.command),['plugin:app|name','plugin:app|version','plugin:app|identifier']);
 await c.open('feedback');assert.equal(calls.at(-1).command,'platform_open_url');assert.deepEqual(calls.at(-1).args,{url:ABOUT_LINKS.feedback.url});
 }finally{delete globalThis.window;}
});
test('failed system browser delivery retains exact link and copy requires a separate action through host command',async()=>{
 const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});if(command==='platform_open_url')throw new Error('denied');}}};
 try{const c=new MobileAboutController(mobileAboutGateway,()=>{});await c.open('repository');
 assert.equal(c.view.failedLink,'repository');assert.deepEqual(calls.map(c=>c.command),['platform_open_url']);
 await c.copyFailedLink();assert.equal(calls.at(-1).command,'platform_clipboard_write');assert.deepEqual(calls.at(-1).args,{text:ABOUT_LINKS.repository.url});assert.match(c.view.linkMessage,/已复制/);
 }finally{delete globalThis.window;}
});
test('partial metadata errors never fall back to a fabricated version and identifier mismatch stays visible',async()=>{
 const g=base();g.version=async()=>{throw new Error('not native');};g.identifier=async()=> 'another.app';
 const c=new MobileAboutController(g,()=>{});await c.load();assert.equal(c.view.name,'Actual Sink');assert.equal(c.view.version,null);assert.equal(c.view.identifier,'another.app');
 assert.ok(c.view.errors.some(e=>e.includes('客户端版本')));assert.ok(c.view.errors.some(e=>e.includes('不一致')));
 assert.equal(unavailableKernelVersions.running.version,null);assert.equal(unavailableKernelVersions.bundled.version,null);
});
test('duplicate external link request is gated and missing clipboard support remains an error',async()=>{
 let resolve;let opens=0;const g=base();g.open=()=>{opens++;return new Promise(r=>resolve=r);};g.copy=async()=>{throw new Error('no clipboard');};
 const c=new MobileAboutController(g,()=>{});const request=c.open('website');await c.open('feedback');assert.equal(opens,1);resolve();await request;
 g.open=async()=>{throw new Error('no browser');};await c.open('feedback');await c.copyFailedLink();assert.match(c.view.linkMessage,/复制失败/);assert.equal(c.view.failedLink,'feedback');
});
test('late metadata publication after disposal cannot modify page state',async()=>{
 let resolve;const g=base();g.version=()=>new Promise(r=>resolve=r);let publications=0;
 const c=new MobileAboutController(g,()=>publications++);const request=c.load();c.dispose();resolve('late');await request;assert.equal(publications,1);assert.equal(c.view.version,null);
});
test('dependency inventory rejects broken material references and preserves unresolved dependencies',async()=>{
 const {parseLicenseInventory}=await import('../src/lib/features/mobile/license-inventory.ts');
 const fixture={schemaVersion:1,target:{platform:'frontend',triple:'browser',profile:'production',features:[]},provenance:{kind:'synthetic-test-fixture'},entries:[{id:'fixture',name:'fixture',source:'fixture',evidence:[],licenseTextIds:[],noticeTextIds:[],missing:['body unavailable'],textStatus:'missing'}],texts:[],gaps:[{reason:'body unavailable'}]};
 assert.equal(parseLicenseInventory(fixture).entries[0].missing[0],'body unavailable');
 assert.throws(()=>parseLicenseInventory({...fixture,schemaVersion:2}));
 assert.throws(()=>parseLicenseInventory({...fixture,entries:[{...fixture.entries[0],licenseTextIds:['absent']}]}));
 const body='Text shown as plain text',hash='a'.repeat(64);
 assert.throws(()=>parseLicenseInventory({...fixture,texts:[{id:'invalid',sha256:hash,text:body,name:'LICENSE',kind:'license',source:'fixture'}]}));
});

test('malformed host app metadata is unavailable rather than crashing or inventing a value',async()=>{
 const g=base();g.name=async()=>null;g.version=async()=>123;g.identifier=async()=> '   ';
 const c=new MobileAboutController(g,()=>{});await c.load();assert.equal(c.view.loading,false);assert.equal(c.view.name,null);assert.equal(c.view.version,null);assert.equal(c.view.identifier,null);assert.equal(c.view.errors.length,3);
});

test('reviewed license text mapping stays distinct from the unavailable manifest declaration',async()=>{
 const {parseLicenseInventory}=await import('../src/lib/features/mobile/license-inventory.ts');
 const cache=JSON.parse(await readFile(new URL('../resources/licenses/frontend-package-materials.json',import.meta.url),'utf8'));
 const p=cache.packages.find(p=>p.identity.name==='svelte-toolbelt'),m=p.materials[0],id=`sha256-${m.sha256}`;
 const row={...p.identity,id:'reviewed-svelte',declaredLicense:null,reviewedLicenseMapping:p.reviewedLicenseMapping,evidence:[],licenseTextIds:[id],noticeTextIds:[],missing:['license-declaration-not-provided'],textStatus:'available'};
 const fixture={schemaVersion:1,target:{platform:'frontend',triple:'browser',profile:'production',features:[]},provenance:{kind:'test-fixture'},entries:[row],texts:[{...m,id}],gaps:[{reason:'license-declaration-not-provided'}]};
 const actual=parseLicenseInventory(fixture);assert.equal(actual.entries[0].declaredLicense,null);assert.equal(actual.entries[0].reviewedLicenseMapping.expression,'MIT');
 for(const change of [r=>r.reviewedLicenseMapping.expression='unknown',r=>r.reviewedLicenseMapping.manifestDeclared=true,r=>r.reviewedLicenseMapping.materialSha256='b'.repeat(64),r=>r.declaredLicense='MIT',r=>r.missing=[]]){const wrong=structuredClone(fixture);change(wrong.entries[0]);assert.throws(()=>parseLicenseInventory(wrong));}
 const ui=await readFile(new URL('../src/lib/components/mobile/MobileAbout.svelte',import.meta.url),'utf8');assert(ui.includes('包 manifest 未声明许可'));assert(ui.includes('entry.reviewedLicenseMapping?.expression'));
});
