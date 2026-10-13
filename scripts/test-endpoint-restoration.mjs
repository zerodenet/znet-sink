import assert from 'node:assert/strict';
import {test} from 'node:test';
import {EndpointSession} from '../src/lib/features/endpoints/session.ts';
import {sourceRestoreReason,restorationMessage,restorationUnresolved} from '../src/lib/features/endpoints/policy.ts';
const target={bindingId:'trusted',endpointId:'opaque:a',profileId:'p',tag:'a',protocol:'wireguard'};
const endpoint=()=>({endpoint_id:'opaque:a',tag:'a',protocol:'wireguard',core_instance_id:'core',config_revision:4,intent_revision:7,generation:1,state:'running',supported:{operations:[],directions:{inbound:false,outbound:true}},counters:{},configuration:{origin:'canonical'}});
const catalog=()=>({profileId:'p',editableEndpointIds:['opaque:a'],localOverrideIds:['opaque:a'],restorationTargets:[target],endpoints:[endpoint()],capabilities:{available:true,features:[],contracts:{},globalLimitations:[]}});
const plan=()=>({planId:'operation',target,sourceDigest:'digest',sourceUpdatedAtUnixMs:'1',previousEffectiveDigest:'before',effectiveDigest:'after',removedFields:['enabled'],runtimeAvailable:false,impact:'only a',clientScope:{profileId:'p',configRevision:'1',coreInstanceId:1},createdAtUnixMs:'1',expiresAtUnixMs:'300001'});
const receipt=(phase='saved_pending')=>({operationId:'operation',target,phase,localSaved:true,runtimeConfirmed:false,applyIdentity:null,endpoint:null,message:'saved only'});
const gateway=()=>({catalog:async()=>catalog(),control:async()=>endpoint(),details:async()=>({}),previewRestoration:async()=>plan(),applyRestoration:async()=>receipt(),reconcileRestoration:async()=>receipt()});

test('tag-only, mismatched protocol and derived identities cannot obtain source restoration',()=>{
 const c=catalog();assert.equal(sourceRestoreReason(c,endpoint()),null);
 c.restorationTargets=[];c.restorationUnavailableReason='missing confirmed source receipt';
 assert.match(sourceRestoreReason(c,endpoint()),/missing confirmed/);
 c.restorationTargets=[target];assert.ok(sourceRestoreReason(c,{...endpoint(),protocol:'different'}));
 c.editableEndpointIds=[];assert.match(sourceRestoreReason(c,endpoint()),/来源声明/);
});
test('offline cached preview submits only fixed plan once and saved pending never reports runtime success',async()=>{
 const g=gateway();const sent=[];g.applyRestoration=async(...args)=>{sent.push(args);return receipt();};
 const session=new EndpointSession(g,()=>{});await session.refresh();session.invalidate();
 const prepared=await session.previewRestoration(endpoint());assert.equal(prepared.runtimeAvailable,false);
 await session.applyRestoration(prepared,'save_pending');await session.applyRestoration(prepared,'save_pending');
 assert.deepEqual(sent,[['operation','save_pending']]);assert.match(session.view.result,/待应用/);assert.equal(session.view.restoration.runtimeConfirmed,false);
});
test('scope invalidation and same-ID new generation block old confirmation before dispatch',async()=>{
 for(const mutate of [s=>s.invalidate(),s=>s.view.catalog.endpoints[0].generation=2]){
  let sent=0;const g=gateway();g.applyRestoration=async()=>{sent++;return receipt();};
  const session=new EndpointSession(g,()=>{});await session.refresh();const prepared=await session.previewRestoration(endpoint());mutate(session);
  await session.applyRestoration(prepared,'save_pending');assert.equal(sent,0);assert.match(session.view.error.message,/变化/);
 }
});
test('foreign preview target is rejected without retaining a callable plan',async()=>{
 const g=gateway();g.previewRestoration=async()=>({...plan(),target:{...target,endpointId:'other'}});
 const session=new EndpointSession(g,()=>{});await session.refresh();assert.equal(await session.previewRestoration(endpoint()),null);assert.match(session.view.error.message,/不匹配/);
});
test('lost apply reply recovers host-filtered unknown journal after revision advance without replay',async()=>{
 const g=gateway();let writes=0;let queries=0;g.catalog=async()=>{const c=catalog();if(writes){c.endpoints[0].config_revision=5;c.restorationTargets=[];}return c;};
 g.applyRestoration=async()=>{writes++;throw new Error('lost acknowledgement');};
 g.restorationReceipts=async()=>writes?[{...receipt('result_unknown'),localSaved:false,message:'apply acknowledgement absent'}]:[];
 g.reconcileRestoration=async id=>{queries++;assert.equal(id,'operation');return {...receipt('result_unknown'),localSaved:false};};
 const session=new EndpointSession(g,()=>{});await session.refresh();const prepared=await session.previewRestoration(endpoint());await session.applyRestoration(prepared,'apply_runtime');
 assert.equal(session.view.restoration.phase,'result_unknown');assert.match(session.view.result,/未确认/);await session.reconcileRestoration();assert.equal(writes,1);assert.equal(queries,1);
});
test('receipt recovery rejects foreign profile, opaque resource or protocol',async()=>{
 for(const override of [{profileId:'other'},{endpointId:'other'},{protocol:'other'}]){
 const g=gateway();g.restorationReceipts=async()=>[{...receipt('result_unknown'),target:{...target,...override}}];
 const session=new EndpointSession(g,()=>{});await session.refresh();assert.equal(session.view.restoration,null);
 }
});
test('contradictory confirmed phase remains unknown until all authoritative facts are present',()=>{
 const r=receipt('confirmed');r.message='success';assert.match(restorationMessage(r),/未确认/);assert.equal(restorationUnresolved(r),true);
 r.runtimeConfirmed=true;r.applyIdentity={coreInstanceId:'core',configRevision:'5'};r.endpoint=endpoint();
 assert.match(restorationMessage(r),/共同确认/);assert.equal(restorationUnresolved(r),false);
});
