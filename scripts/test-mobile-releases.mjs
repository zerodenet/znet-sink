import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import ts from 'typescript';
import {compareAppVersions} from '../src/lib/services/app-update-policy.ts';
function compiled(file){const api={};const js=ts.transpileModule(readFileSync(new URL(file,import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;new Function('exports','require',js)(api,()=>({compareAppVersions}));return api;}
const {MobileReleaseBrowser,releaseComparison}=compiled('../src/lib/features/mobile/release-browser.ts');
const {MobileReleasePreferences}=compiled('../src/lib/features/mobile/release-preferences.ts');
const catalog=(source='client',channel='stable',version='1.0.1')=>({source,channel,platform:'android',checkedAtUnixMs:'1788697000000',releases:[{version,tagName:`v${version}`,notes:'real notes'}],targetStatus:'installation-not-adapted',targetReason:'No platform installer'});
const snapshot={preferences:{clientAutoCheck:false,clientChannel:'stable',kernelChannel:'stable'},expectedRevision:'original-full-config',schedulerState:'not-adapted'};

test('catalog compares semantic versions without claiming unknown current or unsupported target is installable',()=>{
 assert.equal(releaseComparison(catalog(),'1.0.0'),'available');assert.equal(releaseComparison(catalog(),'1.0.1'),'no-update');assert.equal(releaseComparison(catalog(),'2.0.0'),'no-update');
 assert.equal(releaseComparison(catalog(),null),'current-unknown');assert.equal(releaseComparison(catalog(),'dev'),'current-unknown');
 assert.equal(releaseComparison(catalog('kernel','test','1.0.1-dev.3'),'1.0.1-dev.2'),'available');
 assert.equal(releaseComparison(catalog('client','stable','9007199254740993.0.0'),'9007199254740992.0.0'),'current-unknown');
 assert.equal(releaseComparison({...catalog(),releases:[]},null),'no-release');assert.equal(releaseComparison(null,'1.0.0'),'unchecked');
});
test('pending old channel cannot deliver after user selects a different channel',async()=>{
 let resolve;const requests=[];const browser=new MobileReleaseBrowser({catalog:(source,channel)=>{requests.push([source,channel]);return new Promise(r=>resolve=r);}},()=>{},'client');
 const pending=browser.check();await browser.check();assert.deepEqual(requests,[['client','stable']]);browser.select('client','preview');resolve(catalog());await pending;
 assert.equal(browser.state.channel,'preview');assert.equal(browser.state.catalog,null);assert.equal(browser.state.phase,'idle');
});
test('catalog failure retains explicitly stale prior result, never becomes no updates or authoritative empty',async()=>{
 let fail=false;const browser=new MobileReleaseBrowser({catalog:async()=>{if(fail)throw new Error('https://private.test/token');return catalog();}},()=>{},'client');await browser.check();fail=true;await browser.check();
 assert.equal(browser.state.phase,'error');assert.equal(browser.state.catalog.releases.length,1);assert.ok(!browser.state.error.includes('private'));assert.ok(!browser.state.error.includes('token'));
});
test('wrong target scope and destroyed UI discard late catalog',async()=>{
 const browser=new MobileReleaseBrowser({catalog:async()=>catalog('kernel')},()=>{},'client');await browser.check();assert.equal(browser.state.phase,'error');assert.equal(browser.state.catalog,null);
 let resolve;const seen=[];const old=new MobileReleaseBrowser({catalog:()=>new Promise(r=>resolve=r)},value=>seen.push(value),'kernel');const pending=old.check();old.destroy();const count=seen.length;resolve(catalog('kernel'));await pending;assert.equal(seen.length,count);
});
test('editing ordinary intent never writes; explicit save submits only preferences plus expected full revision once',async()=>{
 const calls=[];const prefs=new MobileReleasePreferences({preferences:async()=>snapshot,savePreferences:async(revision,draft)=>{calls.push([revision,draft]);return {...snapshot,preferences:draft,expectedRevision:'after'};}},()=>{});
 await prefs.read();prefs.edit({...prefs.state.draft,clientAutoCheck:true,clientChannel:'test'});assert.deepEqual(calls,[]);await prefs.save();
 assert.equal(calls.length,1);assert.equal(calls[0][0],'original-full-config');assert.deepEqual(calls[0][1],{clientAutoCheck:true,clientChannel:'test',kernelChannel:'stable'});
 assert.match(prefs.state.notice,/只保存意图/);assert.equal(prefs.state.snapshot.schedulerState,'not-adapted');
});
test('unknown preference save stays fixed, no replay, read reports authoritative current without invented success',async()=>{
 let writes=0,reads=0;const prefs=new MobileReleasePreferences({preferences:async()=>{reads++;return snapshot;},savePreferences:async()=>{writes++;throw {code:'connection_closed',message:'secret raw URL'};}},()=>{});
 await prefs.read();prefs.edit({...prefs.state.draft,clientAutoCheck:true});await prefs.save();assert.equal(prefs.state.phase,'unknown');assert.ok(!prefs.state.error.includes('secret'));prefs.edit({...snapshot.preferences});await prefs.save();assert.equal(writes,1);
 await prefs.read();assert.equal(reads,2);assert.equal(writes,1);assert.equal(prefs.state.draft.clientAutoCheck,false);assert.match(prefs.state.notice,/原保存回执仍未确认/);
});
test('conflict keeps preference draft; late acknowledgement cannot update destroyed page',async()=>{
 const prefs=new MobileReleasePreferences({preferences:async()=>snapshot,savePreferences:async()=>{throw {code:'conflict',message:'raw secret'};}},()=>{});await prefs.read();prefs.edit({...prefs.state.draft,clientChannel:'preview'});await prefs.save();assert.equal(prefs.state.phase,'error');assert.equal(prefs.state.draft.clientChannel,'preview');assert.ok(!prefs.state.error.includes('secret'));
 let finish;const seen=[];const other=new MobileReleasePreferences({preferences:async()=>snapshot,savePreferences:()=>new Promise(r=>finish=r)},value=>seen.push(value));await other.read();const pending=other.save();other.destroy();const count=seen.length;finish(snapshot);await pending;assert.equal(seen.length,count);
});
