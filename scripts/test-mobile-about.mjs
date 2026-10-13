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
test('license imports bind to actual repo and installed-package full texts with explicit incomplete inventory',async()=>{
 const service=await readFile(new URL('../src/lib/services/mobile-license-notices.ts',import.meta.url),'utf8');
 for(const path of ['LICENSE','node_modules/@lucide/svelte/LICENSE','node_modules/@fontsource-variable/inter/LICENSE','node_modules/flag-icons/LICENSE','src-tauri/resources/builtin-rules/LICENSE.meta-rules-dat']){
  assert.ok(service.includes(`../../../${path}?raw`));assert.ok((await readFile(new URL(`../${path}`,import.meta.url),'utf8')).length>300);
 }
 assert.match(service,/尚未完整提供/);assert.match(service,/license:lucide\.license/);assert.match(service,/license:inter\.license/);assert.match(service,/license:rules\.sourceLicense/);
 const component=await readFile(new URL('../src/lib/components/mobile/MobileAbout.svelte',import.meta.url),'utf8');assert.ok(component.includes('{projectLicense.text}'));assert.ok(component.includes('{notice.text}'));assert.ok(!component.includes('updater'));
});

test('malformed host app metadata is unavailable rather than crashing or inventing a value',async()=>{
 const g=base();g.name=async()=>null;g.version=async()=>123;g.identifier=async()=> '   ';
 const c=new MobileAboutController(g,()=>{});await c.load();assert.equal(c.view.loading,false);assert.equal(c.view.name,null);assert.equal(c.view.version,null);assert.equal(c.view.identifier,null);assert.equal(c.view.errors.length,3);
});
