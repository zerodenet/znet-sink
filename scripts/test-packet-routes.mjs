import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PacketSession } from '../src/lib/features/packet/session.ts';
import { closeReason, packetTime } from '../src/lib/features/packet/types.ts';
const row = () => ({ routeId:'opaque:packet:1',coreInstanceId:'core-1',configRevision:'18446744073709551615',inboundTag:'tun',outboundTag:'wg',endpoints:[{endpointId:'device',generation:'18446744073709551615'}],source:'::1',destination:'::2',ipProtocol:58,translated:false,startedAtUnixMs:'18446744073709551615',state:'active' });
const snapshot = () => ({ supported:true,canClose:true,reason:null,closeReason:null,profileId:'p1',coreInstanceId:'core-1',configRevision:'18446744073709551615',observedAtUnixMs:'10',routes:[row()],total:1,nextOffset:null });
const deferred = () => { let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve}; };
function setup() { const g={snapshot:async()=>snapshot(),detail:async()=>row(),close:async()=>({status:'confirmed',message:'closed',route:{...row(),state:'closed'}})};const s=new PacketSession(g,()=>{});return {g,s}; }
test('admission requires declared support, admin, current instance and revision; u64 is never rounded',()=>{
 const r=row(),c=snapshot();assert.equal(closeReason(c,r,false),null);
 assert.match(closeReason({...c,supported:false,reason:'not supported'},r,false),/not supported/);
 assert.match(closeReason({...c,canClose:false,closeReason:'admin required'},r,false),/admin/);
 assert.match(closeReason(c,{...r,coreInstanceId:'core-2'},false),/变化/);
 assert.match(closeReason(c,{...r,configRevision:'18446744073709551614'},false),/变化/);
 assert.match(closeReason(c,r,true),/未确认/);
 assert.match(closeReason(c,{...r,state:'closing'},false),/正在关闭/);
 assert.equal(packetTime(r.startedAtUnixMs),'18446744073709551615');
});
test('confirmation target stays fixed and an added same-tag resource does not expand the close',async()=>{
 const {s,g}=setup();await s.refresh();const target=s.target(row());let submitted;g.close=async t=>{submitted=t;return {status:'confirmed',message:'closed',route:null};};
 s.view.snapshot.routes.push({...row(),routeId:'new'});await s.close(target);
 assert.deepEqual(submitted,target);assert.equal(submitted.routeId,'opaque:packet:1');
});
test('late reads and old confirmation cannot affect a replacement profile/instance',async()=>{
 const {s,g}=setup();await s.refresh();const target=s.target(row());const read=deferred();g.snapshot=()=>read.promise;const running=s.refresh();s.invalidate();read.resolve(snapshot());await running;
 assert.equal(s.view.snapshot,null);let calls=0;g.close=async()=>{calls++;};await s.close(target);assert.equal(calls,0);
 const newer={...snapshot(),profileId:'p2',coreInstanceId:'core-2',routes:[{...row(),coreInstanceId:'core-2'}]};g.snapshot=async()=>newer;await s.refresh();await s.close(target);assert.equal(calls,0);
});
test('unknown close is visible and is not automatically replayed after refresh',async()=>{
 const {s,g}=setup();await s.refresh();const target=s.target(row());let calls=0;g.close=async()=>{calls++;return {status:'unconfirmed',message:'result unknown',route:null};};await s.close(target);
 assert.deepEqual(s.view.unconfirmed,[target.routeId]);assert.equal(s.view.message,'result unknown');await s.refresh();await s.close(target);assert.equal(calls,1);
});
test('explicit rejection retains error and confirmed route until a fresh read, never local fake closure',async()=>{
 const {s,g}=setup();await s.refresh();g.close=async()=>{throw {code:'permission_denied',message:'denied'};};await s.close(s.target(row()));
 assert.equal(s.view.error.code,'permission_denied');assert.equal(s.view.snapshot.routes[0].state,'active');assert.deepEqual(s.view.unconfirmed,[]);
});
test('read failure retains timestamped snapshot as stale and blocks close',async()=>{
 const {s,g}=setup();await s.refresh();g.snapshot=async()=>{throw new Error('offline');};await s.refresh();assert.equal(s.view.stale,true);assert.equal(s.view.snapshot.observedAtUnixMs,'10');assert.equal(s.target(row()),null);
});
test('leaving page discards late control publication but does not simulate cancellation',async()=>{
 const {s,g}=setup();await s.refresh();const pending=deferred();let calls=0;g.close=()=>{calls++;return pending.promise;};const running=s.close(s.target(row()));s.dispose();pending.resolve({status:'confirmed',message:'closed',route:null});await running;assert.equal(calls,1);assert.equal(s.view.message,null);
});
test('periodic snapshot refresh keeps selected details current and clears a removed lease',async()=>{
 const {s,g}=setup();await s.refresh();await s.inspect(row());assert.equal(s.view.detail.routeId,row().routeId);
 g.snapshot=async()=>({...snapshot(),routes:[{...row(),state:'closing'}]});await s.refresh();assert.equal(s.view.detail.state,'closing');
 g.snapshot=async()=>({...snapshot(),routes:[],total:0});await s.refresh();assert.equal(s.view.detail,null);
});
test('background reads retain explicit action refusal and report a separate observation error',async()=>{
 const {s,g}=setup();await s.refresh();g.close=async()=>{throw {code:'permission_denied',message:'denied'};};await s.close(s.target(row()));
 g.snapshot=async()=>{throw new Error('offline');};await s.refresh();assert.equal(s.view.error.code,'permission_denied');assert.equal(s.view.refreshError.message,'offline');
});
