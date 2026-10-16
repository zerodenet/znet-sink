import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import ts from 'typescript';
import {compareAppVersions} from '../src/lib/services/app-update-policy.ts';
function compiled(file){const api={};const js=ts.transpileModule(readFileSync(new URL(file,import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;new Function('exports','require',js)(api,path=>path.endsWith('release-browser') ? browserApi : ({compareAppVersions}));return api;}
const browserApi=compiled('../src/lib/features/mobile/release-browser.ts');
const {MobileReleaseBrowser,releaseComparison}=browserApi;
const {MobileReleasePreferences}=compiled('../src/lib/features/mobile/release-preferences.ts');
const {MobileReleaseSchedule,releaseTime}=compiled('../src/lib/features/mobile/release-schedule.ts');
const id='0123456789abcdef0123456789abcdef';
const prefs={clientAutoCheck:false,clientChannel:'stable',kernelChannel:'stable'};
const snapshot={preferences:prefs,expectedRevision:'original-full-config',schedulerState:'native-foreground',operation:null};
const catalog=(source='client',channel='stable',version='1.0.1')=>({source,channel,platform:'android',checkedAtUnixMs:'1788697000000',catalogLimit:30,releases:[{version,tagName:`v${version}`,channel,notes:'real notes',kernelChannel:source==='client'?null:({stable:'stable',preview:'beta',test:'nightly'})[channel],notesTruncated:false,publishedAt:null,releaseUrl:`https://github.com/zerodenet/${source==='client'?'znet-sink':'core'}/releases/tag/v${version}`}],targetStatus:source==='client'?'installation-not-adapted':'bundled-via-client',targetReason:'No platform installer',requestId:'request-1',responseStatus:200,reusedValidatedCache:false});
const operation=(preferences={...prefs,clientAutoCheck:true},state='saved-awaiting-confirm')=>({operationId:id,expectedRevision:snapshot.expectedRevision,preferences,state,acknowledged:state==='confirmed',reviewBinding:'original-reviewed-binding',reason:null,receipt:{ownerOperation:id,beforeEpoch:'1',afterEpoch:'2'}});
const ack=(op=operation())=>({...snapshot,schedulerState:op.acknowledged?'native-foreground':'preferences-unconfirmed',preferences:op.preferences,expectedRevision:'after',operation:op});
function gateway(overrides={}) { return {preferences:async()=>snapshot,savePreferences:async(_r,draft)=>ack(operation(draft)),preferenceOperation:async()=>operation(),confirmPreferences:async()=>ack(operation(undefined,'confirmed')),...overrides}; }
function controller(g= gateway(),notify=()=>{}){return new MobileReleasePreferences(g,notify,()=>id);}
const schedule=(extra={})=>({hostSessionId:'host-a',revision:'1',foreground:true,checking:false,enabled:true,clientChannel:'stable',lastAttemptedAtUnixMs:'1788697000000',lastSuccessAtUnixMs:'1788697000000',nextDueAtUnixMs:'1788718600000',retryAtUnixMs:null,outcome:'success',reason:null,storageError:false,cachedCatalog:catalog(),cacheCurrent:true,preferenceOperation:null,foregroundScheduling:'native-lifecycle',backgroundScheduling:'not-adapted',...extra});

test('catalog compares semantic versions without claiming unknown current or unsupported target is installable',()=>{
 assert.equal(releaseComparison(catalog(),'1.0.0'),'available');assert.equal(releaseComparison(catalog(),'1.0.1'),'no-update');assert.equal(releaseComparison(catalog(),'2.0.0'),'no-update');
 assert.equal(releaseComparison(catalog(),null),'current-unknown');assert.equal(releaseComparison(catalog(),'dev'),'current-unknown');assert.equal(releaseComparison(catalog('kernel','test','1.0.1-dev.3'),'1.0.1-dev.2'),'available');
 assert.equal(releaseComparison(catalog('client','stable','9007199254740993.0.0'),'9007199254740992.0.0'),'current-unknown');assert.equal(releaseComparison({...catalog(),releases:[]},null),'no-release');assert.equal(releaseComparison(null,'1.0.0'),'unchecked');
});
test('pending old channel cannot deliver after user selects another channel; duplicate UI click does not submit twice',async()=>{
 let resolve;const requests=[];const browser=new MobileReleaseBrowser({catalog:(source,channel)=>{requests.push([source,channel]);return new Promise(r=>resolve=r);}},()=>{},'client');
 const pending=browser.check();await browser.check();assert.deepEqual(requests,[['client','stable']]);browser.select('client','preview');resolve(catalog());await pending;assert.equal(browser.state.catalog,null);assert.equal(browser.state.phase,'idle');
});
test('catalog failure retains explicitly historical prior result and fixed error',async()=>{
 let fail=false;const browser=new MobileReleaseBrowser({catalog:async()=>{if(fail)throw Error('https://private.test/token');return catalog();}},()=>{},'client');await browser.check();fail=true;await browser.check();assert.equal(browser.state.phase,'error');assert.equal(browser.state.catalog.releases.length,1);assert.equal(browser.state.catalogCurrent,false);assert.ok(!browser.state.error.includes('token'));
});
test('wrong scope, malformed HTTP acknowledgement and destroyed UI discard results',async()=>{
 for (const bad of [catalog('kernel'),{...catalog(),responseStatus:304,reusedValidatedCache:false},{...catalog(),requestId:''}]) {const b=new MobileReleaseBrowser({catalog:async()=>bad},()=>{},'client');await b.check();assert.equal(b.state.phase,'error');assert.equal(b.state.catalog,null);}
 let finish;const seen=[];const b=new MobileReleaseBrowser({catalog:()=>new Promise(r=>finish=r)},v=>seen.push(v),'kernel');const p=b.check();b.destroy();const count=seen.length;finish(catalog('kernel'));await p;assert.equal(seen.length,count);
});
test('only exact client/channel cached facts hydrate browser, stale cache cannot report no update and manual in-flight is not replaced',async()=>{
 let finish;const b=new MobileReleaseBrowser({catalog:()=>new Promise(r=>finish=r)},()=>{},'client');b.acceptCached(catalog('kernel'),true);assert.equal(b.state.catalog,null);b.acceptCached(catalog(),false);assert.equal(b.state.catalogCurrent,false);const p=b.check();b.acceptCached({...catalog(),releases:[]},true);assert.equal(b.state.catalog.releases.length,1);finish({...catalog(),responseStatus:304,reusedValidatedCache:true});await p;assert.equal(b.state.catalogCurrent,true);assert.equal(b.state.catalog.responseStatus,304);
});
test('edit never writes, save freezes original ID/preferences/revision and acknowledges the one owned receipt once',async()=>{
 const calls=[];let saved;const c=controller(gateway({savePreferences:async(r,d,o)=>{calls.push(['save',r,d,o]);saved=operation(d);return ack(saved);},confirmPreferences:async(o,binding)=>{calls.push(['confirm',o,binding]);return ack({...saved,state:'confirmed',acknowledged:true});}}));await c.read();c.edit({...prefs,clientAutoCheck:true,clientChannel:'test'});assert.equal(calls.length,0);await c.save();assert.equal(calls.length,2);assert.deepEqual(calls[0],['save',snapshot.expectedRevision,{clientAutoCheck:true,clientChannel:'test',kernelChannel:'stable'},id]);assert.deepEqual(calls[1],['confirm',id,'original-reviewed-binding']);assert.equal(c.state.phase,'ready');assert.match(c.state.notice,/前台到期检查/);await c.confirm();assert.equal(calls.length,2);
});
test('lost save reply keeps original draft and operation; read/current equality cannot remove unknown; explicit receipt check then confirm',async()=>{
 let writes=0,confirms=0;const c=controller(gateway({savePreferences:async()=>{writes++;throw {code:'connection_closed',message:'secret'};},preferences:async()=>({...snapshot,preferences:{...prefs,clientAutoCheck:true}}),confirmPreferences:async()=>{confirms++;return ack(operation(undefined,'confirmed'));}}));await c.read();c.edit({...prefs,clientAutoCheck:false});await c.save(); // Create an actual difference from the current true value.
 assert.equal(c.state.phase,'unknown');await c.read();assert.equal(c.state.phase,'unknown');assert.equal(c.state.draft.clientAutoCheck,false);await c.save();assert.equal(writes,1);
 // A mismatching original intent cannot be borrowed from status.
 await c.checkOperation();assert.equal(c.state.phase,'unknown');assert.equal(confirms,0);assert.match(c.state.error,/核对失败/);
});
test('lost reply status is read only until explicit original acknowledgement',async()=>{
 let writes=0,confirms=0;const c=controller(gateway({savePreferences:async()=>{writes++;throw Error('secret');},confirmPreferences:async()=>{confirms++;return ack(operation(undefined,'confirmed'));}}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();await c.checkOperation();assert.equal(writes,1);assert.equal(confirms,0);assert.equal(c.state.phase,'unknown');assert.equal(c.state.operation.state,'saved-awaiting-confirm');await c.confirm();assert.equal(confirms,1);assert.equal(c.state.phase,'ready');
});
test('lost confirm reply cannot be confirmed again; only original confirmed receipt clears unknown',async()=>{
 let confirms=0;const c=controller(gateway({confirmPreferences:async()=>{confirms++;throw Error('lost');},preferenceOperation:async()=>operation(undefined,'confirmed'),preferences:async()=> confirms ? ack(operation(undefined,'confirmed')) : snapshot}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(c.state.phase,'unknown');assert.equal(c.state.confirmAttempted,true);await c.confirm();assert.equal(confirms,1);await c.checkOperation();assert.equal(c.state.phase,'ready');assert.equal(confirms,1);
});
test('page remount discovers unacknowledged original operation without save replay',async()=>{
 let writes=0;const c=controller(gateway({preferences:async()=>ack(operation()),savePreferences:async()=>{writes++;throw Error();}}));await c.read();assert.equal(c.state.phase,'unknown');assert.equal(c.state.operation.operationId,id);assert.equal(c.state.draft.clientAutoCheck,true);await c.save();assert.equal(writes,0);
});
test('dirty read preserves draft and original CAS baseline; rejected save preserves draft',async()=>{
 let current=snapshot;const c=controller(gateway({preferences:async()=>current,savePreferences:async()=>{throw {code:'conflict',message:'secret'};}}));await c.read();c.edit({...prefs,clientChannel:'preview'});current={...snapshot,expectedRevision:'new',preferences:{...prefs,clientAutoCheck:true}};await c.read();assert.equal(c.state.draft.clientChannel,'preview');assert.equal(c.state.snapshot.expectedRevision,snapshot.expectedRevision);assert.equal(c.state.currentFacts.expectedRevision,'new');await c.save();assert.equal(c.state.phase,'ready');assert.equal(c.state.draft.clientChannel,'preview');assert.ok(!c.state.error.includes('secret'));
});
test('malformed/borrowed acknowledgement cannot claim saved or call confirm',async()=>{
 for (const bad of [ack({...operation(),operationId:'a'.repeat(32)}),ack({...operation(),receipt:null}),ack({...operation(),state:'prepared',receipt:null})]) {let confirm=0;const c=controller(gateway({savePreferences:async()=>bad,confirmPreferences:async()=>{confirm++;return ack();}}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(c.state.phase,'unknown');assert.equal(confirm,0);}
});
test('late save response after destruction never confirms or publishes',async()=>{
 let finish,confirms=0;const seen=[];const c=controller(gateway({savePreferences:()=>new Promise(r=>finish=r),confirmPreferences:async()=>{confirms++;return ack();}}),v=>seen.push(v));await c.read();c.edit({...prefs,clientAutoCheck:true});const p=c.save();c.destroy();const n=seen.length;finish(ack());await p;assert.equal(seen.length,n);assert.equal(confirms,0);
});
test('schedule observation only reads host facts, coalesces wakeups and performs a second fresh read',async()=>{
 const resolvers=[];let calls=0;const s=new MobileReleaseSchedule({schedule:()=>{calls++;return new Promise(r=>resolvers.push(r));}},()=>{});const p=s.read();await s.read();await s.read();assert.equal(calls,1);resolvers.shift()(schedule());await new Promise(r=>setImmediate(r));assert.equal(calls,2);resolvers.shift()(schedule({revision:'2',checking:true}));await p;assert.equal(s.state.snapshot.checking,true);assert.equal(s.state.snapshot.revision,'2');
});
test('schedule stale revisions and failed reads preserve historical facts; host replacement accepts fresh identity',async()=>{
 let value=schedule();const s=new MobileReleaseSchedule({schedule:async()=>{if(value instanceof Error)throw value;return value;}},()=>{});await s.read();value=schedule({revision:'0'});await s.read();assert.equal(s.state.phase,'error');assert.equal(s.state.snapshot.revision,'1');value=Error('secret token');await s.read();assert.equal(s.state.phase,'error');assert.ok(!s.state.error.includes('token'));value=schedule({hostSessionId:'host-new',revision:'0',enabled:false});await s.read();assert.equal(s.state.snapshot.hostSessionId,'host-new');
});
test('schedule dispose ignores late facts; cross-channel cached data and unsafe revisions reject',async()=>{
 for(const bad of [schedule({revision:'9007199254740993000000'}),schedule({cachedCatalog:catalog('client','test')}),schedule({hostSessionId:''})]) {const s=new MobileReleaseSchedule({schedule:async()=>bad},()=>{});await s.read();assert.equal(s.state.phase,'error');assert.equal(s.state.snapshot,null);}
 let finish;const seen=[];const s=new MobileReleaseSchedule({schedule:()=>new Promise(r=>finish=r)},v=>seen.push(v));const p=s.read();s.destroy();const n=seen.length;finish(schedule());await p;assert.equal(seen.length,n);
});
test('human times reject unsafe and invalid dates instead of showing implementation timestamps',()=>{assert.equal(releaseTime('18446744073709551615'),'—');assert.equal(releaseTime('NaN'),'—');assert.equal(releaseTime(null),'—');assert.notEqual(releaseTime('1788697000000'),'—');});
test('manual failure cannot be erased by old scheduler cache or same prior successful request',async()=>{
 let failed=false;const b=new MobileReleaseBrowser({catalog:async()=>{if(failed)throw {code:'http403'};return catalog();}},()=>{},'client');await b.check();failed=true;await b.check();const error=b.state.error;b.acceptCached(catalog(),false);assert.equal(b.state.phase,'error');assert.equal(b.state.error,error);assert.equal(b.state.catalogCurrent,false);b.acceptCached(catalog(),true);assert.equal(b.state.error,error);b.acceptCached({...catalog(),requestId:'new-confirmed',checkedAtUnixMs:'1788697000001'},true);assert.equal(b.state.phase,'ready');assert.equal(b.state.error,null);
});
test('catalog wire guards reject malformed platform, dates, URLs, notes fields, kernel mapping, and missing cache metadata',async()=>{
 const invalid=[{platform:'unknown'},{targetStatus:'installed'},{checkedAtUnixMs:'18446744073709551616'}, {releases:[{...catalog().releases[0],notesTruncated:undefined}]},{releases:[{...catalog().releases[0],publishedAt:{secret:'no'}}]},{releases:[{...catalog().releases[0],releaseUrl:'https://github.com.evil/zerodenet/znet-sink/releases/tag/v1.0.1'}]},{releases:[{...catalog().releases[0],notes:'密'.repeat(22000)}]},{releases:[{...catalog().releases[0],kernelChannel:'stable'}]}];
 for(const patch of invalid){const b=new MobileReleaseBrowser({catalog:async()=>({...catalog(),...patch})},()=>{},'client');await b.check();assert.equal(b.state.catalog,null);assert.equal(b.state.phase,'error');}
 for(const patch of [{cachedCatalog:null,cacheCurrent:true},{cachedCatalog:undefined,cacheCurrent:false},{lastSuccessAtUnixMs:'18446744073709551616'}]){const s=new MobileReleaseSchedule({schedule:async()=>schedule(patch)},()=>{});await s.read();assert.equal(s.state.phase,'error');}
});
test('superseded scheduler await never publishes old current facts, including an old failed await',async()=>{
 for(const failure of [false,true]){const pending=[];const seen=[];const s=new MobileReleaseSchedule({schedule:()=>new Promise((resolve,reject)=>pending.push({resolve,reject}))},v=>seen.push(v));const p=s.read();await s.read();if(failure)pending[0].reject(Error('old'));else pending[0].resolve(schedule({reason:'old-scope'}));await new Promise(r=>setImmediate(r));assert.equal(seen.some(v=>v.phase==='ready'||v.phase==='error'),false);pending[1].resolve(schedule({hostSessionId:'new-host',reason:'new-scope'}));await p;assert.equal(seen.filter(v=>v.phase==='ready').length,1);assert.equal(s.state.snapshot.hostSessionId,'new-host');}
});
test('missing original review prevents acknowledgement, fresh status binding must be used exactly once',async()=>{
 let confirms=[];let currentOp=operation();const c=controller(gateway({savePreferences:async()=>ack({...operation(),reviewBinding:null}),preferenceOperation:async()=>({...currentOp,reviewBinding:'new-host-review'}),confirmPreferences:async(id,binding)=>{confirms.push([id,binding]);return ack({...currentOp,state:'confirmed',acknowledged:true});}}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(c.state.phase,'unknown');assert.equal(confirms.length,0);await c.checkOperation();await c.confirm();assert.deepEqual(confirms,[[id,'new-host-review']]);
});
test('actual preference scheduler states remain readable and normal awaiting-confirm response is accepted',async()=>{
 for(const schedulerState of ['native-foreground','not-native-host','storage-error','preferences-unconfirmed']){const c=controller(gateway({preferences:async()=>({...snapshot,schedulerState})}));await c.read();assert.equal(c.state.phase,'ready');assert.equal(c.state.snapshot.schedulerState,schedulerState);}
 let confirmed=0;const c=controller(gateway({savePreferences:async()=>({...ack(),schedulerState:'preferences-unconfirmed'}),confirmPreferences:async()=>{confirmed++;return ack(operation(undefined,'confirmed'));}}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(confirmed,1);assert.equal(c.state.phase,'ready');
});
test('borrowed or malformed epoch receipts and inconsistent acknowledgement flags cannot be confirmed',async()=>{
 for(const op of [{...operation(),acknowledged:true},{...operation(),receipt:{...operation().receipt,afterEpoch:'4'}},{...operation(),receipt:{...operation().receipt,beforeEpoch:'18446744073709551615',afterEpoch:'18446744073709551616'}}]){let confirms=0;const c=controller(gateway({savePreferences:async()=>ack(op),confirmPreferences:async()=>{confirms++;return ack(operation(undefined,'confirmed'));}}));await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(c.state.phase,'unknown');assert.equal(confirms,0);}
});
test('explicit discard reload changes stale CAS only after user action; refresh/cancel retain the original dirty draft',async()=>{
 let current=snapshot;const saves=[];let owned;
 const c=controller(gateway({preferences:async()=>current,savePreferences:async(r,d,id)=>{saves.push([r,d,id]);if(r!==current.expectedRevision)throw {code:'conflict'};owned={...operation(d),expectedRevision:r};return {...ack(owned),operation:owned};},confirmPreferences:async()=>({...ack({...owned,state:'confirmed',acknowledged:true})})}));
 await c.read();c.edit({...prefs,clientChannel:'preview'});current={...snapshot,expectedRevision:'external',preferences:{...prefs,clientAutoCheck:true}};await c.save();assert.equal(c.state.phase,'ready');assert.equal(saves.length,1);
 await c.read();assert.equal(c.state.draft.clientChannel,'preview');assert.equal(c.state.snapshot.expectedRevision,snapshot.expectedRevision); // No discard/cancel side effect.
 await c.discardAndReload();assert.equal(c.state.snapshot.expectedRevision,'external');assert.equal(c.state.draft.clientChannel,'stable');assert.equal(c.state.draft.clientAutoCheck,true);assert.equal(saves.length,1);
 c.edit({...c.state.draft,clientChannel:'test'});await c.save();assert.equal(c.state.phase,'ready');assert.equal(saves.length,2);assert.equal(saves[1][0],'external');
});
test('explicit discard is unavailable during unknown/save/confirm and failed reload cannot leave a writable stale baseline',async()=>{
 let reads=0,finish;
 const c=controller(gateway({preferences:async()=>{reads++;return snapshot;},savePreferences:()=>new Promise(r=>finish=r)}));await c.read();c.edit({...prefs,clientAutoCheck:true});const p=c.save();await c.discardAndReload();assert.equal(reads,1);assert.equal(c.state.phase,'saving');finish(ack({...operation(),receipt:null}));await p;assert.equal(c.state.phase,'unknown');await c.discardAndReload();assert.equal(reads,1);assert.equal(c.state.phase,'unknown');
 let fail=false;const d=controller(gateway({preferences:async()=>{if(fail)throw Error('no read');return snapshot;}}));await d.read();d.edit({...prefs,clientChannel:'test'});fail=true;await d.discardAndReload();assert.equal(d.state.phase,'error');assert.equal(d.state.snapshot,null);await d.save();assert.equal(d.state.snapshot,null);
});
test('explicit preflight confirmation rejection clears old review; status must provide a fresh review before another user acknowledgement',async()=>{
 const confirmations=[];let writes=0;const original=operation();
 const c=controller(gateway({savePreferences:async()=>{writes++;return ack(original);},preferenceOperation:async()=>({...original,reviewBinding:'fresh-reviewed-binding'}),confirmPreferences:async(id,binding)=>{confirmations.push([id,binding]);if(binding==='original-reviewed-binding')throw {code:'release_check_invalidated'};return ack({...original,state:'confirmed',acknowledged:true});}}));
 await c.read();c.edit({...prefs,clientAutoCheck:true});await c.save();assert.equal(c.state.phase,'unknown');assert.equal(c.state.operation.reviewBinding,null);assert.equal(c.state.confirmAttempted,false);assert.match(c.state.error,/先只读核对/);await c.confirm();assert.equal(confirmations.length,1);await c.checkOperation();assert.equal(confirmations.length,1);assert.equal(c.state.operation.reviewBinding,'fresh-reviewed-binding');await c.confirm();assert.equal(c.state.phase,'ready');assert.equal(writes,1);assert.deepEqual(confirmations,[[id,'original-reviewed-binding'],[id,'fresh-reviewed-binding']]);
});
test('explicit navigation discard while a dirty read is pending retires that read and really discards the draft',async()=>{
 let reads=0,finish;const c=controller(gateway({preferences:()=>++reads===1?Promise.resolve(snapshot):new Promise(resolve=>finish=resolve)}));await c.read();c.edit({...prefs,clientChannel:'preview'});const pending=c.read();assert.equal(c.state.phase,'loading');c.discardDraft();assert.equal(c.state.phase,'ready');assert.deepEqual(c.state.draft,prefs);finish({...snapshot,expectedRevision:'late-external',preferences:{...prefs,clientAutoCheck:true}});await pending;assert.equal(c.state.snapshot.expectedRevision,snapshot.expectedRevision);assert.deepEqual(c.state.draft,prefs);
});
