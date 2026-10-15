import assert from 'node:assert/strict';
import { test } from 'node:test';
import { packetGateway } from '../src/lib/features/packet/client.ts';
import { PacketSession } from '../src/lib/features/packet/session.ts';
import { closeReason, packetTime, physicalKey, validatePacketPage } from '../src/lib/features/packet/types.ts';
const row = () => ({ routeId:'opaque:packet:1',coreInstanceId:'core-1',configRevision:'18446744073709551615',inboundTag:'tun',outboundTag:'wg',endpoints:[{endpointId:'device',generation:'18446744073709551615'}],source:'::1',destination:'::2',ipProtocol:58,translated:false,startedAtUnixMs:'18446744073709551615',state:'active' });
const snapshot = () => ({ offset:0,limit:500,clientScopeGeneration:1,hostSessionId:'fixture-session',closeRecords:[],closeLedgerRecovery:{state:'ready',reason:null,recoveredCount:0},blockedTargets:[],closeLedgerFull:false,supported:true,canClose:true,reason:null,closeReason:null,profileId:'p1',coreInstanceId:'core-1',configRevision:'18446744073709551615',observedAtUnixMs:'10',routes:[row()],total:1,nextOffset:null });
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


test('real paging sends fixed offset, limit and original identity; final item and previous page are reachable',async()=>{
 const {s,g}=setup();const calls=[];
 g.snapshot=async query=>{calls.push(query);const offset=query.offset;return {...snapshot(),offset,total:1001,nextOffset:offset<1000?offset+500:null,routes:Array.from({length:offset<1000?500:1},(_,i)=>({...row(),routeId:`packet:${offset+i}`}))};};
 await s.refresh();await s.next();assert.equal(s.view.snapshot.offset,500);await s.next();assert.equal(s.view.snapshot.routes[0].routeId,'packet:1000');assert.equal(s.view.snapshot.nextOffset,null);await s.previous();assert.equal(s.view.snapshot.offset,500);await s.refresh();assert.equal(calls.at(-1).offset,500);assert.deepEqual(calls[1].expected,{profileId:'p1',coreInstanceId:'core-1',configRevision:'18446744073709551615',clientScopeGeneration:1,hostSessionId:'fixture-session'});
});
test('failed next page preserves timestamped old page stale, retries exact offset, and never closes old selection',async()=>{
 const {s,g}=setup();g.snapshot=async query=>{if(query.offset===500)throw new Error('offline');return {...snapshot(),total:501,nextOffset:500,routes:Array.from({length:500},(_,i)=>({...row(),routeId:`packet:${i}`}))};};
 await s.refresh();const original=s.target(s.view.snapshot.routes[0]);await s.next();assert.equal(s.view.snapshot.offset,0);assert.equal(s.view.failedOffset,500);assert.equal(s.view.stale,true);let closes=0;g.close=async()=>{closes++;};await s.close(original);assert.equal(closes,0);
 let retried;g.snapshot=async query=>{retried=query;return {...snapshot(),offset:500,total:501,routes:[{...row(),routeId:'packet:500'}]};};await s.retry();assert.equal(retried.offset,500);assert.equal(s.view.snapshot.offset,500);assert.equal(s.view.stale,false);
});
test('a late subsequent page and fixed confirmation cannot reach a replacement client generation',async()=>{
 const {s,g}=setup();g.snapshot=async()=>({...snapshot(),total:501,nextOffset:500,routes:Array.from({length:500},(_,i)=>({...row(),routeId:`packet:${i}`}))});await s.refresh();const old=s.target(s.view.snapshot.routes[0]);const read=deferred();g.snapshot=()=>read.promise;const running=s.next();s.invalidate();read.resolve({...snapshot(),offset:500,total:501,routes:[row()]});await running;assert.equal(s.view.snapshot,null);
 g.snapshot=async()=>({...snapshot(),clientScopeGeneration:3});await s.refresh();let closes=0;g.close=async()=>{closes++;};await s.close(old);assert.equal(closes,0);
});
test('unknown physical target survives another page, profile ABA, and local invalidation without replay',async()=>{
 const {s,g}=setup();let closes=0;g.close=async()=>{closes++;return {status:'unconfirmed',message:'unknown',route:null};};await s.refresh();await s.close(s.target(row()));
 g.snapshot=async()=>({...snapshot(),offset:500,total:501,routes:[{...row(),routeId:'other'}]});s.view.snapshot={...s.view.snapshot,total:501,nextOffset:500,routes:Array.from({length:500},(_,i)=>({...row(),routeId:i===0?row().routeId:`packet:${i}`}))};await s.next();assert.equal(s.view.blockedTargets.length,1);
 s.invalidate();g.snapshot=async()=>({...snapshot(),profileId:'p2',clientScopeGeneration:2});await s.refresh();s.invalidate();g.snapshot=async()=>({...snapshot(),clientScopeGeneration:3});await s.refresh();assert.match(s.reason(row()),/原物理会话/);await s.close(s.target(row()));assert.equal(closes,1);
});
test('only matching fixed ended detail facts release a barrier; ordinary not_found cannot do so',async()=>{
 const {s,g}=setup();await s.refresh();g.close=async()=>({status:'unconfirmed',message:'unknown',route:null});await s.close(s.target(row()));
 g.detail=async()=>{throw {code:'not_found',details:null};};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,1);
 g.detail=async()=>{throw {code:'not_found',details:{packetTargetEnded:{...row(),routeId:'other'}}};};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,1);
 g.detail=async()=>{throw {code:'not_found',details:{packetTargetEnded:row()}};};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,0);
});
test('128 unresolved targets block new close risk without preventing subsequent page reads',async()=>{
 const {s,g}=setup();const blocked=Array.from({length:128},(_,i)=>({...row(),routeId:`unknown:${i}`}));g.snapshot=async query=>({...snapshot(),offset:query.offset,total:501,nextOffset:query.offset===0?500:null,blockedTargets:blocked,closeLedgerFull:true,routes:query.offset===0?Array.from({length:500},(_,i)=>({...row(),routeId:`packet:${i}`})):[row()]});await s.refresh();let closes=0;g.close=async()=>{closes++;};await s.close(s.target(s.view.snapshot.routes[0]));assert.equal(closes,0);await s.next();assert.equal(s.view.snapshot.offset,500);assert.match(s.reason(row()),/预算/);
});
test('empty offsets beyond total are valid; nonempty end past total, invalid counts and stalled cursors are refused',()=>{
 const q={offset:500,limit:500,expected:null};assert.doesNotThrow(()=>validatePacketPage({...snapshot(),offset:500,total:0,routes:[]},q));
 for(const value of [
  {...snapshot(),offset:500,total:550,nextOffset:null,routes:Array.from({length:100},(_,i)=>({...row(),routeId:`overflow:${i}`}))},
  {...snapshot(),offset:500,total:1000,nextOffset:500},
  {...snapshot(),offset:500,total:4294967296,nextOffset:501},
  {...snapshot(),offset:500,total:Number.MAX_SAFE_INTEGER+1,nextOffset:501},
  {...snapshot(),offset:500,clientScopeGeneration:0},
  {...snapshot(),offset:500,clientScopeGeneration:Number.MAX_SAFE_INTEGER+1},
 ])assert.throws(()=>validatePacketPage(value,q));
 assert.notEqual(physicalKey({...row(),configRevision:'18446744073709551614'}),physicalKey(row()));
});
test('unconfirmed in-flight result cannot be forgotten by a local invalidation before receipt',async()=>{
 const {s,g}=setup();await s.refresh();const pending=deferred();let closes=0;g.close=()=>{closes++;return pending.promise;};const running=s.close(s.target(row()));s.invalidate();pending.resolve({status:'unconfirmed',message:'unknown',route:null});await running;g.snapshot=async()=>({...snapshot(),clientScopeGeneration:3});await s.refresh();await s.close(s.target(row()));assert.equal(closes,1);assert.equal(s.view.blockedTargets.length,1);
});


test('actual Tauri Packet gateway carries only the fixed bounded query and original target DTO',async()=>{
 const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return snapshot();}}};
 try{const query={offset:500,limit:500,expected:{profileId:'p1',coreInstanceId:'core-1',configRevision:'18446744073709551615',clientScopeGeneration:3,hostSessionId:'fixture-session'}};const target={...row(),profileId:'p1',clientScopeGeneration:3,hostSessionId:'fixture-session'};await packetGateway.snapshot(query);await packetGateway.detail(target);await packetGateway.close(target);
 assert.deepEqual(calls,[{command:'gui_packet_routes_snapshot',args:{query}},{command:'gui_packet_route_detail',args:{target}},{command:'gui_close_packet_route',args:{target}}]);
 }finally{delete globalThis.window;}
});

const closeRecord=(operationId='original-close')=>({target:{routeId:row().routeId,coreInstanceId:row().coreInstanceId,configRevision:row().configRevision},operationId,originProcessId:'retired-host-process',originProfileId:'old-profile',originClientScopeGeneration:'18446744073709551615',reservedAtUnixMs:'18446744073709551615',recovered:true});
function recoveryFixture() {
 let records=[closeRecord()],hostSessionId='reconstructed-host',blocked=false;const calls=[];
 const gateway={snapshot:async query=>{calls.push(['snapshot',query]);return {...snapshot(),hostSessionId,closeRecords:records,blockedTargets:records.map(record=>record.target),closeLedgerRecovery:{state:blocked?'blocked':'ready',reason:blocked?'persistent storage unavailable':null,recoveredCount:blocked?null:records.length},canClose:!blocked,closeLedgerFull:blocked}},detail:async target=>{calls.push(['detail',target]);return {...row(),closeOperationId:target.closeOperationId,state:'active'}},close:async target=>{calls.push(['close',target]);return {status:'unconfirmed',message:'unknown',route:null,operationId:'new-close'}}};
 return {gateway,calls,setRecords:value=>{records=value},setHost:value=>{hostSessionId=value},block:()=>{blocked=true},session:()=>new PacketSession(gateway,()=>{})};
}
test('reconstructed host ledger restores only historical fixed metadata and never dispatches close or detail automatically',async()=>{
 const f=recoveryFixture(),s=f.session();await s.refresh();assert.equal(s.view.blockedTargets.length,1);assert.equal(s.view.snapshot.closeRecords[0].originClientScopeGeneration,'18446744073709551615');assert.equal(s.view.snapshot.closeRecords[0].recovered,true);assert.equal(f.calls.filter(c=>c[0]!=='snapshot').length,0);await s.close(s.target(row()));assert.equal(f.calls.filter(c=>c[0]==='close').length,0);
 s.dispose();const recreated=f.session();await recreated.refresh();assert.equal(recreated.view.blockedTargets.length,1);assert.equal(recreated.view.snapshot.hostSessionId,'reconstructed-host');
});
test('explicit readonly recovery carries fresh host session plus exact original operation and never replays close',async()=>{
 const f=recoveryFixture(),s=f.session();await s.refresh();await s.reconcile(row());const target=f.calls.find(c=>c[0]==='detail')[1];assert.equal(target.hostSessionId,'reconstructed-host');assert.equal(target.closeOperationId,'original-close');assert.equal(target.configRevision,'18446744073709551615');assert.equal(s.view.blockedTargets.length,1);assert.equal(f.calls.filter(c=>c[0]==='close').length,0);
});
test('only an exact original operation ended receipt may clear a restored record',async()=>{
 const f=recoveryFixture(),s=f.session();await s.refresh();f.gateway.detail=async()=>({...row(),state:'closed',closeOperationId:'different-close'});await s.reconcile(row());assert.equal(s.view.blockedTargets.length,1);
 f.gateway.detail=async()=>{throw {code:'not_found',details:{packetTargetEnded:row(),closeOperationId:'different-close'}}};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,1);
 f.gateway.detail=async target=>{f.setRecords([]);return {...row(),state:'closed',closeOperationId:target.closeOperationId}};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,0);assert.equal(s.view.snapshot.closeRecords.length,0);
});
test('late original detail cannot clear a replacement close operation on the same physical target',async()=>{
 const f=recoveryFixture(),s=f.session();await s.refresh();const d=deferred();f.gateway.detail=()=>d.promise;const checking=s.reconcile(row());s.invalidate();f.setRecords([closeRecord('replacement-close')]);await s.refresh();d.resolve({...row(),state:'closed',closeOperationId:'original-close'});await checking;assert.equal(s.view.blockedTargets.length,1);assert.equal(s.view.snapshot.closeRecords[0].operationId,'replacement-close');assert.equal(s.view.detail,null);
});
test('persistent ledger failure preserves readonly data and cannot be mistaken for successful reconciliation or budget exhaustion',async()=>{
 const f=recoveryFixture();f.block();const s=f.session();await s.refresh();assert.equal(s.view.stale,false);assert.equal(s.view.snapshot.closeLedgerRecovery.recoveredCount,null);await s.close(s.target(row()));assert.equal(f.calls.filter(c=>c[0]==='close').length,0);
 f.gateway.detail=async()=>{throw {code:'ledger_unavailable',message:'ended fact cannot be durably committed'}};await s.reconcile(row());assert.equal(s.view.blockedTargets.length,1);assert.equal(s.view.message,null);assert.equal(s.view.error.code,'ledger_unavailable');
});
test('new host session with repeated numeric generation rejects old page and confirmation until explicit fresh discovery',async()=>{
 const f=recoveryFixture();f.setRecords([]);const s=f.session();await s.refresh();const old=s.target(row());f.setHost('replacement-host');await s.refresh();assert.equal(s.view.stale,true);assert.equal(s.view.snapshot.hostSessionId,'reconstructed-host');await s.close(old);assert.equal(f.calls.filter(c=>c[0]==='close').length,0);
 await s.first();assert.equal(s.view.snapshot.hostSessionId,'replacement-host');assert.equal(s.view.stale,false);await s.close(old);assert.equal(f.calls.filter(c=>c[0]==='close').length,0);assert.equal(s.target(row()).hostSessionId,'replacement-host');
});
test('missing host session or malformed recovered metadata fails closed instead of inventing a current identity',async()=>{
 for(const patch of [{hostSessionId:undefined},{closeRecords:[{...closeRecord(),originClientScopeGeneration:'18446744073709551616'}]},{closeLedgerRecovery:{state:'ready',reason:null,recoveredCount:null}}]){const {s,g}=setup();g.snapshot=async()=>({...snapshot(),...patch});await s.refresh();assert.equal(s.view.stale,true);assert.equal(s.target(row()),null);assert.ok(s.view.refreshError);}
});
test('a restoration token is not accepted as a new close even for a matching active route',async()=>{
 const {s,g}=setup();await s.refresh();let closes=0;g.close=async()=>{closes++};await s.close({...s.target(row()),closeOperationId:'historical-close'});assert.equal(closes,0);assert.match(s.view.error.message,/恢复记录/);
});
