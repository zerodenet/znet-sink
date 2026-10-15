import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import ts from 'typescript';
function service() {
 const source=readFileSync(new URL('../src/lib/services/config-workspace.svelte.ts',import.meta.url),'utf8');
 const compiled=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const exports={};
 new Function('require','exports','$state',compiled)(name=>{
  if(name.endsWith('/core'))return {};
  if(name.includes('proxy-config-signal'))return {proxyConfigSignal:{onActiveChanged(){}}};
  if(name.includes('config-validation'))return {normalizeConfigValidationError:error=>error?.details?.errors?.map(e=>({fieldPath:e.field_path,message:e.message}))??[]};
  if(name.includes('toast'))return {success(){},warning(){}};
  throw new Error(name);
 },exports,value=>value);
 return exports.ConfigWorkspaceService;
}
const Service=service();
const text='{ "route":{"final":{"type":"direct"}}, "runtime":{"udp_upstream_idle_timeout_seconds":18446744073709551615} }';
const snapshot=(binding='source-a',readOnly=false)=>({profileId:'a',profileName:'a',binding:readOnly?null:binding,sourceUpdatedAtUnixMs:'1',sourceJson:readOnly?null:text,localEditsJson:readOnly?null:'{}',effectiveJson:readOnly?null:text,readOnlyReason:readOnly?'插件托管，不提供正文':null,pendingApply:true,runtime:{running:false,confirmed:false,effectiveDigest:'same',reason:'运行待确认'}});
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});return {promise,resolve,reject};};
function fixture() {
 let current=snapshot(), id=0;const calls=[];
 const ports={snapshot:async()=>{calls.push(['snapshot']);return current},format:async t=>{calls.push(['format',t]);return t.trim()},validate:async input=>{calls.push(['validate',input]);return {sourceJson:input.sourceJson,effectiveJson:input.sourceJson}},plan:async input=>{calls.push(['plan',input]);return {planId:`plan-${++id}`,binding:input.binding,sourceJson:input.sourceJson,effectiveJson:input.sourceJson,effectiveDigest:'same',hotReload:[],requiresRestart:[],valid:true,warnings:[],errors:[],expiresAtUnixMs:'9999999999999',runtimeValidated:true}},save:async input=>{calls.push(['save',input]);current={...current,binding:'saved',sourceJson:input.sourceJson};return {state:'saved_pending_apply',snapshot:current}},apply:async input=>{calls.push(['apply',input]);throw new Error('lost response')},operation:async planId=>{calls.push(['operation',planId]);return {planId,state:'submitted_unknown'}},discard:async id=>calls.push(['discard',id])};
 const editor=new Service(ports);return {editor,ports,calls,set:s=>{current=s},get:()=>current};
}
test('native text including u64 MAX survives load, format, reset, plan and local save without JS serialization',async()=>{
 const f=fixture();await f.editor.load();assert.equal(f.editor.draftJson,text);f.editor.updateDraft(text+'\n');await f.editor.format();assert.equal(f.editor.draftJson,text);f.editor.updateDraft('bad');f.editor.reset();assert.equal(f.editor.draftJson,text);
 await f.editor.planApply();await f.editor.saveLocal();for(const [,input] of f.calls.filter(c=>['plan','save'].includes(c[0])))assert.equal(input.sourceJson,text);
 assert.equal(f.editor.phase,'saved');assert.equal(f.editor.pendingApply,true);assert.equal(f.editor.runtime.confirmed,false);assert.equal(f.calls.filter(c=>c[0]==='apply').length,0);
});
test('external same timestamp source change preserves dirty draft and requires explicit discard refresh',async()=>{
 const f=fixture();await f.editor.load();f.editor.updateDraft(text+'  ');await f.editor.planApply();f.set(snapshot('source-b'));await f.editor.reconcileExternalSource();assert.equal(f.editor.draftJson,text+'  ');assert.equal(f.editor.conflict,true);assert.equal(f.editor.planResult,null);assert.equal(await f.editor.saveLocal(),false);assert.equal(await f.editor.load(),false);await f.editor.load(true);assert.equal(f.editor.draftJson,text);assert.equal(f.editor.conflict,false);
});
test('managed snapshots provide no three-view text and cannot plan, save or apply',async()=>{
 const f=fixture();f.set(snapshot('managed',true));await f.editor.load();assert.equal(f.editor.sourceJson,'');assert.equal(f.editor.localEditsJson,'');assert.equal(f.editor.effectiveJson,'');assert.equal(await f.editor.planApply(),false);assert.equal(await f.editor.saveLocal(),false);assert.equal(await f.editor.apply(),false);assert.equal(f.calls.filter(c=>['plan','save','apply'].includes(c[0])).length,0);
});
test('late load and validation never overwrite newer user input',async()=>{
 const f=fixture();const read=deferred();f.ports.snapshot=()=>read.promise;const loading=f.editor.load();f.editor.updateDraft(text+'  ');read.resolve(snapshot());assert.equal(await loading,false);assert.equal(f.editor.draftJson,text+'  ');
 const g=fixture();await g.editor.load();const validate=deferred();g.ports.validate=()=>validate.promise;const checking=g.editor.validate();g.editor.updateDraft(text+'\n');validate.resolve({sourceJson:'old',effectiveJson:'old'});assert.equal(await checking,false);assert.notEqual(g.editor.effectiveJson,'old');
});
test('late plan is discarded and cannot become executable after input change',async()=>{
 const f=fixture();await f.editor.load();const d=deferred();f.ports.plan=()=>d.promise;const pending=f.editor.planApply();f.editor.updateDraft(text+'\n');d.resolve({planId:'old',binding:'source-a',effectiveJson:'old'});assert.equal(await pending,false);assert.equal(f.editor.planResult,null);assert.deepEqual(f.calls.at(-1),['discard','old']);
});
test('late save leaves new input intact and never declares it saved',async()=>{
 const f=fixture();await f.editor.load();const d=deferred();f.ports.save=()=>d.promise;const saving=f.editor.saveLocal();f.editor.updateDraft(text+'\n');d.resolve({state:'saved_pending_apply',snapshot:snapshot('saved')});assert.equal(await saving,false);assert.equal(f.editor.draftJson,text+'\n');assert.equal(f.editor.dirty,true);assert.notEqual(f.editor.phase,'saved');
});
test('native field validation failure preserves original input and safe field path',async()=>{
 const f=fixture();await f.editor.load();f.editor.updateDraft(text+'\n');f.ports.validate=async()=>{throw {message:'原生校验失败',details:{errors:[{field_path:'runtime.dns',message:'引用不合法'}]}}};assert.equal(await f.editor.validate(),false);assert.equal(f.editor.validationErrors[0].fieldPath,'runtime.dns');assert.equal(f.editor.draftJson,text+'\n');
});
test('lost apply never replans, reapplies, or accepts an old equal-digest workspace receipt',async()=>{
 const f=fixture();await f.editor.load();await f.editor.apply();assert.equal(f.editor.phase,'unknown');assert.equal(f.editor.operationId,'plan-1');assert.equal(await f.editor.saveLocal(),false);
 f.set({...snapshot(),runtime:{confirmed:true},lastTransaction:{effectiveDigest:'same'}});await f.editor.apply();await f.editor.checkOperation();assert.equal(f.editor.phase,'unknown');assert.equal(f.calls.filter(c=>c[0]==='plan').length,1);assert.equal(f.calls.filter(c=>c[0]==='apply').length,1);assert.deepEqual(f.calls.filter(c=>c[0]==='operation').map(c=>c[1]),['plan-1','plan-1']);
});
test('only an original operation receipt may complete and unrelated identity stays unknown',async()=>{
 const f=fixture();await f.editor.load();await f.editor.apply();f.ports.operation=async()=>({planId:'unrelated',state:'confirmed',receipt:{completedAtUnixMs:5}});assert.equal(await f.editor.checkOperation(),false);assert.equal(f.editor.operationId,'plan-1');
 f.ports.operation=async id=>({planId:id,state:'confirmed',receipt:{state:'confirmed',completedAtUnixMs:5,transactionId:'18446744073709551615',runtimeIdentity:{configRevision:'18446744073709551615'}}});f.set({...snapshot('after'),runtime:{confirmed:true}});assert.equal(await f.editor.checkOperation(),true);assert.equal(f.editor.phase,'applied');assert.equal(f.editor.lastAppliedAt,5);assert.equal(f.calls.filter(c=>c[0]==='apply').length,1);
});
test('offline and actual restart impact stay visible without attempting runtime work',async()=>{
 for(const unavailable of ['内核未运行','安全启动通道尚未适配']){const f=fixture();await f.editor.load();const plan=f.ports.plan;f.ports.plan=async input=>({...await plan(input),applyUnavailableReason:unavailable,runtimeValidated:false,requiresRestart:[{section:'api',tags:[],detail:'重启影响'}]});assert.equal(await f.editor.apply(),false);assert.equal(f.editor.planResult.requiresRestart.length,1);assert.equal(f.calls.filter(c=>c[0]==='apply').length,0);assert.equal(await f.editor.saveLocal(),true);}
});
test('source switch during in-flight apply keeps new input and never publishes late receipt',async()=>{
 const f=fixture();await f.editor.load();const d=deferred();f.ports.apply=()=>d.promise;const pending=f.editor.apply();await new Promise(r=>setImmediate(r));f.editor.updateDraft(text+'\n');f.set(snapshot('replacement'));await f.editor.reconcileExternalSource();d.resolve({planId:'plan-1',state:'confirmed',receipt:{completedAtUnixMs:50}});assert.equal(await pending,false);assert.equal(f.editor.draftJson,text+'\n');assert.equal(f.editor.lastAppliedAt,null);assert.equal(f.editor.conflict,true);
});

test('an old source read cannot replace input or publish over a newer save in flight',async()=>{
 const f=fixture();await f.editor.load();const read=deferred(),saved=deferred();f.ports.snapshot=()=>read.promise;const loading=f.editor.load(true);
 assert.equal(await f.editor.saveLocal(),false);assert.equal(await f.editor.planApply(),false);assert.equal(f.calls.filter(c=>['save','plan'].includes(c[0])).length,0);
 f.editor.updateDraft(text+'  ');f.ports.save=()=>saved.promise;const saving=f.editor.saveLocal();read.resolve(snapshot('old'));assert.equal(await loading,false);assert.equal(f.editor.draftJson,text+'  ');saved.resolve({state:'saved_pending_apply',snapshot:{...snapshot('saved'),sourceJson:text+'  '}});await saving;assert.equal(f.editor.draftJson,text+'  ');assert.equal(f.editor.phase,'saved');
});
test('post-confirmation snapshot failure cannot overwrite a later edit or save result',async()=>{
 const f=fixture();await f.editor.load();await f.editor.apply();const refresh=deferred();f.ports.operation=async id=>({planId:id,state:'confirmed',receipt:{state:'confirmed',completedAtUnixMs:5}});f.ports.snapshot=()=>refresh.promise;
 const checking=f.editor.checkOperation();await new Promise(r=>setImmediate(r));f.editor.updateDraft(text+'\n ');await f.editor.saveLocal();assert.equal(f.editor.phase,'saved');refresh.reject(new Error('old read failed'));assert.equal(await checking,false);assert.equal(f.editor.phase,'saved');assert.equal(f.editor.lastError,null);
});
test('late original-operation query failure does not replace newer edit feedback',async()=>{
 const f=fixture();await f.editor.load();await f.editor.apply();const d=deferred();f.ports.operation=()=>d.promise;const checking=f.editor.checkOperation();f.editor.updateDraft(text+'\n ');f.editor.lastError='new feedback';d.reject(new Error('old failed'));await checking;assert.equal(f.editor.lastError,'new feedback');assert.equal(f.editor.draftJson,text+'\n ');
});
test('late save receipt cannot overwrite newer feedback',async()=>{
 const f=fixture();await f.editor.load();const d=deferred();f.ports.save=()=>d.promise;const saving=f.editor.saveLocal();f.editor.updateDraft(text+'\n ');f.editor.lastError='new feedback';d.resolve({state:'saved_pending_apply',snapshot:snapshot('old')});await saving;assert.equal(f.editor.lastError,'new feedback');assert.equal(f.editor.draftJson,text+'\n ');
});
test('a typed pre-side-effect rejection releases only the original plan and requires a new explicit action',async()=>{
 const f=fixture();await f.editor.load();f.ports.apply=async input=>{f.calls.push(['apply',input]);throw {code:'workspace_preflight_rejected',message:'未发送变更'}};
 assert.equal(await f.editor.apply(),false);assert.equal(f.editor.operationId,null);assert.equal(f.editor.planResult,null);assert.equal(f.calls.filter(c=>c[0]==='apply').length,1);assert.equal(f.calls.filter(c=>c[0]==='plan').length,1);
 await f.editor.planApply();assert.equal(f.calls.filter(c=>c[0]==='plan').length,2);assert.equal(f.calls.filter(c=>c[0]==='apply').length,1);
});
test('runtime-confirmed but local-unpublished snapshot restores the original readonly recheck entry',async()=>{
 const f=fixture();f.set({...snapshot(),pendingOperation:{planId:'original',state:'runtime_confirmed_pending_local',receipt:{state:'runtime_confirmed',transactionId:'18446744073709551615'}}});await f.editor.load();assert.equal(f.editor.phase,'unknown');assert.equal(f.editor.operationId,'original');assert.equal(await f.editor.saveLocal(),false);assert.equal(await f.editor.planApply(),false);
 f.ports.operation=async id=>({planId:id,state:'runtime_confirmed_pending_local',receipt:{state:'runtime_confirmed'}});assert.equal(await f.editor.checkOperation(),false);assert.equal(f.editor.lastAppliedAt,null);
});
