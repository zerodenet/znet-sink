import assert from 'node:assert/strict';
import { test } from 'node:test';
import { RemovalSession } from '../src/lib/features/profile-removal/session.ts';
import { removalInput } from '../src/lib/features/profile-removal/types.ts';
import { createRemovalGateway } from '../src/lib/features/profile-removal/gateway.ts';
const preview=()=>({targetId:'target',targetName:'source',targetVersion:'18446744073709551615',active:true,sourceType:'subscription',guard:'digest',subscriptions:[{id:'sub',name:'related'}],replacements:[{id:'first',name:'first',reason:null},{id:'chosen',name:'chosen',reason:null},{id:'managed',name:'plugin',reason:'protected'}],runtime:{state:'running',coreInstanceId:'core-a',configRevision:'18446744073709551615',managedPid:2},blockedReason:null,pending:false,overrideCount:1});
const result=(changes={})=>({status:'confirmed',message:'confirmed',removedId:'target',replacementId:'chosen',runtimeStopped:false,...changes});
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function setup(){const g={preview:async()=>preview(),commit:async()=>result(),reconcile:async()=>result()};const removed=[];const s=new RemovalSession(g,()=>{},r=>removed.push(r));return {g,s,removed};}
test('no automatic first replacement and guarded/protected/native-active targets refuse admission',()=>{
 const p=preview();assert.equal(removalInput(p,{kind:'replace',replacementId:''}),null);assert.equal(removalInput(p,{kind:'replace',replacementId:'managed'}),null);
 assert.equal(removalInput({...p,blockedReason:'native VPN unknown'},{kind:'stop'}),null);assert.equal(removalInput({...p,pending:true},{kind:'stop'}),null);
 assert.equal(removalInput(p,{kind:'inactive'}),null);assert.deepEqual(removalInput(p,{kind:'replace',replacementId:'chosen'}),{targetId:'target',guard:'digest',action:{kind:'replace',replacementId:'chosen'}});
});
test('confirmation fixes target, replacement and preview guard even if UI selection changes after dispatch',async()=>{
 const {s,g}=setup();await s.open('target');const wait=deferred();let captured;g.commit=async input=>{captured=structuredClone(input);return wait.promise;};
 const choice={kind:'replace',replacementId:'chosen'};const running=s.commit(choice);choice.replacementId='first';await s.commit({kind:'stop'});
 assert.equal(captured.action.replacementId,'chosen');assert.equal(captured.guard,'digest');assert.equal(captured.targetId,'target');wait.resolve(result());await running;
});
test('old preview and late deletion results cannot publish to a new target/dialog',async()=>{
 const {s,g,removed}=setup();const first=deferred();g.preview=id=>id==='target'?first.promise:Promise.resolve({...preview(),targetId:id});const reading=s.open('target');await s.open('other');first.resolve(preview());await reading;assert.equal(s.view.preview.targetId,'other');
 const wait=deferred();g.commit=()=>wait.promise;const running=s.commit({kind:'replace',replacementId:'chosen'});s.close();wait.resolve(result({removedId:'other'}));await running;assert.equal(removed.length,0);assert.equal(s.view.targetId,null);
});
test('unknown outcome does not notify success or automatically replay; reconciliation is read-only gateway operation',async()=>{
 const {s,g,removed}=setup();await s.open('target');let commits=0,rechecks=0;g.commit=async()=>{commits++;return result({status:'unconfirmed',removedId:null,replacementId:null});};g.reconcile=async()=>{rechecks++;return result({status:'unconfirmed',removedId:null});};
 await s.commit({kind:'replace',replacementId:'chosen'});await s.commit({kind:'replace',replacementId:'chosen'});await s.reconcile();assert.equal(commits,1);assert.equal(rechecks,1);assert.equal(removed.length,0);assert.equal(s.view.unconfirmed,true);
});
test('transport failure is unknown while authoritative refusal retains preview and permits corrected confirmation',async()=>{
 const {s,g}=setup();await s.open('target');g.commit=async()=>{throw {code:'timeout',message:'lost'};};await s.commit({kind:'stop'});assert.equal(s.view.unconfirmed,true);
 await s.open('target');g.commit=async()=>{throw {code:'invalid_argument',message:'replacement invalid'};};await s.commit({kind:'replace',replacementId:'chosen'});assert.equal(s.view.unconfirmed,false);assert.equal(s.view.preview.targetId,'target');assert.equal(s.view.error.message,'replacement invalid');
});
test('success requires same target/chosen replacement and confirmed actual stop',async()=>{
 const {s,g,removed}=setup();await s.open('target');g.commit=async()=>result({replacementId:'first'});await s.commit({kind:'replace',replacementId:'chosen'});assert.equal(removed.length,0);assert.equal(s.view.unconfirmed,true);
 await s.open('target');g.commit=async()=>result({replacementId:null,runtimeStopped:false});await s.commit({kind:'stop'});assert.equal(removed.length,0);
 await s.open('target');g.commit=async()=>result({replacementId:null,runtimeStopped:true});await s.commit({kind:'stop'});assert.equal(removed.length,1);
});
test('inactive local delete is allowed with unknown native VPN state without a VPN stop claim',async()=>{
 const {s,g,removed}=setup();g.preview=async()=>({...preview(),active:false,runtime:{...preview().runtime,state:'platform_unknown'}});g.commit=async()=>result({replacementId:null});await s.open('target');await s.commit({kind:'inactive'});assert.equal(removed.length,1);assert.equal(removed[0].runtimeStopped,false);
});
test('persistent pending preview forces reconcile; proven not-deleted requires fresh confirmation',async()=>{
 const {s,g}=setup();g.preview=async()=>({...preview(),pending:true,blockedReason:'pending'});await s.open('target');assert.equal(s.view.unconfirmed,true);
 let commits=0;g.commit=async()=>{commits++;return result();};await s.commit({kind:'stop'});assert.equal(commits,0);
 g.preview=async()=>preview();g.reconcile=async()=>result({status:'not_deleted',removedId:null});await s.reconcile();assert.equal(s.view.unconfirmed,false);assert.equal(s.view.result.status,'not_deleted');assert.equal(commits,0);
});
test('shared change signals fire only for authoritative matching deletion receipts',async()=>{
 let signals=0,response=result();const calls=[];const gateway=createRemovalGateway(async(command,args)=>{calls.push({command,args});return response;},()=>signals++);
 const input=removalInput(preview(),{kind:'replace',replacementId:'chosen'});await gateway.commit(input);assert.equal(signals,1);
 response=result({status:'unconfirmed',removedId:null});await gateway.commit(input);assert.equal(signals,1);
 response=result({replacementId:'first'});await gateway.commit(input);assert.equal(signals,1);
 response=result({removedId:'other'});await gateway.reconcile('target');assert.equal(signals,1);
 response=result();await gateway.reconcile('target');assert.equal(signals,2);assert.equal(calls.at(-1).command,'profile_removal_reconcile');
});
test('version or scope conflict invalidates the old confirmation until a new preview',async()=>{
 const {s,g}=setup();await s.open('target');let calls=0;g.commit=async()=>{calls++;throw {code:'conflict',message:'source changed'};};await s.commit({kind:'stop'});assert.equal(s.view.preview,null);await s.commit({kind:'stop'});assert.equal(calls,1);assert.equal(s.view.unconfirmed,false);
});
