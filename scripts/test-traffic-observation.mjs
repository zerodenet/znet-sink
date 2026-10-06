import test from 'node:test';
import assert from 'node:assert/strict';
import { FixtureGateway, sample, scopes, discovery } from '../tests/fixtures/traffic.ts';
import { TrafficSession, MAX_HISTORY_SCOPES } from '../src/lib/features/traffic/session.ts';
import { TrafficWorkspace } from '../src/lib/features/traffic/workspace.ts';
import { scopeKey } from '../src/lib/features/traffic/types.ts';
import { inventory } from '../src/lib/features/traffic/inventory.ts';
import { observe, metric, rebaseline, formatBytes, MAX_POINTS } from '../src/lib/features/traffic/history.ts';
import { page, snapshot, u64 } from '../src/lib/features/traffic/wire.ts';
import { supported, samplingSupported, resetReason } from '../src/lib/features/traffic/policy.ts';
const flush=async()=>{for(let i=0;i<20;i++)await new Promise(r=>setImmediate(r));};
async function started(gateway=new FixtureGateway(),now){const session=new TrafficSession(gateway,()=>{},now);await session.start();await flush();return session;}
test('u64, null, zero and all five opaque identities remain distinct',()=>{
 assert.equal(u64('18446744073709551615'),'18446744073709551615');assert.throws(()=>u64(9007199254740992));assert.throws(()=>u64('18446744073709551616'));
 assert.equal(new Set(scopes.map(scopeKey)).size,5);assert.notEqual(scopeKey({kind:'inbound',tag:'x'}),scopeKey({kind:'outbound',tag:'x'}));
 const s=sample();s.planes[0].counters.bytes_down='0';assert.equal(metric(s.planes[0],'bytes_down'),0n);assert.equal(metric(s.planes[0],'errors'),null);assert.equal(metric(s.planes[2],'rx_bytes'),null);
 assert.equal(formatBytes(null),'—');assert.equal(formatBytes(0n),'0 B');assert.equal(snapshot(s).config_revision,'9007199254740993');
 assert.throws(()=>page({core_instance_id:'a',config_revision:'1',registry_revision:'1',sampled_at_unix_ms:'1',scopes:[s],total:1,next_offset:null}));
});
test('rates use scope monotonic time and epoch, never wall time or other planes',()=>{
 const a=sample();a.planes[0].counters.bytes_down='90071992547409930';let first=observe(undefined,a,true);
 const b=structuredClone(a);b.sampled_at_monotonic_ns=String(BigInt(a.sampled_at_monotonic_ns)+2_000_000_000n);b.sampled_at_unix_ms=String(BigInt(a.sampled_at_unix_ms)+1n);b.planes[0].counters.bytes_down='90071992547410930';
 const second=observe(first,b,true);assert.equal(second.rates['flow.bytes_down'],500);assert.equal(second.rates['inner.rx_bytes'],0);assert.equal(second.rates['outer.rx_bytes'],null);
 assert.equal(observe(rebaseline(second),b,true).rates['flow.bytes_down'],null);
 for(const field of ['stats_epoch','core_instance_id','generation']){const next=structuredClone(b);next[field]='other';assert.equal(observe(second,next,true).rates['flow.bytes_down'],null);}
 const drop=structuredClone(b);drop.planes[0].counters.bytes_down='1';drop.sampled_at_monotonic_ns=String(BigInt(b.sampled_at_monotonic_ns)+1n);assert.equal(observe(second,drop,true).rates['flow.bytes_down'],null);
});
test('histories are bounded by time and point count',()=>{
 let row;let s=sample();for(let i=0;i<300;i++){s=structuredClone(s);s.sampled_at_monotonic_ns=String(BigInt(s.sampled_at_monotonic_ns)+100_000_000n);s.sampled_at_unix_ms=String(BigInt(s.sampled_at_unix_ms)+100n);row=observe(row,s,true);}
 assert.equal(row.points.length,MAX_POINTS);s.sampled_at_unix_ms=String(BigInt(s.sampled_at_unix_ms)+121_000n);assert.equal(observe(row,s,true).points.length,1);
});
test('capabilities gate old kernels, query-only transport and Admin/reset metrics',()=>{
 const d=discovery();assert(supported(d));assert(samplingSupported(d));assert.equal(resetReason(d,sample()),null);
 d.capabilities.trafficStatistics.automatic_sampling=false;assert(!samplingSupported(d));d.admin=false;assert.equal(resetReason(d,sample()),'需要管理员权限');
 d.capabilities.features=[];assert(!supported(d));assert(resetReason(d,sample()));
});
test('multi-page inventory echoes exact revision conditions and restarts whole round on conflict',async()=>{
 const g=new FixtureGateway();g.pageLimit=2;g.conflictOnce=true;
 const all=await inventory(g,2,()=>true);assert.equal(all.scopes.length,5);assert.equal(g.queries.filter(q=>q.offset===0).length,2);assert.equal(g.queries[1].expected_config_revision,'9007199254740993');assert.equal(g.queries[1].expected_registry_revision,'1');assert.equal(g.queries.at(-1).expected_registry_revision,'2');
 const bad=new FixtureGateway();bad.page=async()=>({core_instance_id:'a',config_revision:'1',registry_revision:'1',sampled_at_unix_ms:'1',scopes:[],total:17000,next_offset:null});await assert.rejects(()=>inventory(bad,64,()=>true));
});
test('sample pages do not delete scopes; only completed inventory does; partial failure preserves inventory',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{
  const key=scopeKey(scopes[0]);s.watch([key]);g.advance([0]);g.push([0]);assert.equal(s.view.order.length,5);assert.equal(s.view.rows[key].rates['flow.bytes_down'],1000);
  const saved=g.page.bind(g);g.page=async()=>{throw{code:'offline'};};await s.refresh();assert.equal(s.view.order.length,5);assert(s.view.stale);
  g.page=saved;g.rows.splice(4,1);g.registry='2';g.advance();await s.refresh();assert.equal(s.view.order.length,4);assert(!s.view.stale);
 }finally{s.dispose();}assert.equal(g.stops,1);
});
test('clear preserves activity, other scopes and real nonzero confirmation; duplicate clicks submit once',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{
  const key=scopeKey(scopes[3]),other=scopeKey(scopes[4]);s.watch([key,other]);g.advance();g.push();const untouched=s.view.rows[other];const plan=s.plan([key]);assert.equal(plan.targets[0].expected_generation,'9007199254740994');
  const first=s.reset(plan);await s.reset(plan);await first;assert.equal(g.commands.length,1);assert.equal(s.view.rows[key].snapshot.planes[1].counters.rx_bytes,'5');assert.equal(s.view.rows[key].snapshot.activity.active_stream_flows,'2');assert.equal(s.view.rows[key].rates['inner.rx_bytes'],null);assert.deepEqual(s.view.rows[other],untouched);
  const noGeneration=s.plan([scopeKey(scopes[0])]);assert.equal('expected_generation' in noGeneration.targets[0],false);
 }finally{s.dispose();}
});
test('lost reset acknowledgement queries recovery, keeps new epoch and never resubmits',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{
  const key=scopeKey(scopes[3]);const old=structuredClone(g.rows[3]);g.mode='lost_ack';await s.reset(s.plan([key]));assert.equal(g.commands.length,1);assert.notEqual(s.view.rows[key].snapshot.stats_epoch,old.stats_epoch);assert.match(s.view.result,/不会自动重试/);
  const epoch=s.view.rows[key].snapshot.stats_epoch;
  g.event({type:'sample',instance:'core-a',payload:{core_instance_id:'core-a',config_revision:old.config_revision,registry_revision:g.registry,sampled_at_unix_ms:old.sampled_at_unix_ms,scopes:[old],total:5,next_offset:null}});
  await flush();assert.equal(s.view.rows[key].snapshot.stats_epoch,epoch);assert.equal(g.commands.length,1);
 }finally{s.dispose();}
});
test('period, generation and instance changes query authority; old events and non-increasing samples cannot roll it back',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{
  const key=scopeKey(scopes[3]),old=structuredClone(g.rows[3]);g.rows[3].generation='9007199254740995';g.rows[3].stats_epoch='z-new';g.advance([3]);g.push([3]);await flush();assert.equal(s.view.rows[key].snapshot.generation,'9007199254740995');assert.equal(s.view.rows[key].rates['inner.rx_bytes'],null);
  g.event({type:'reset',instance:'core-a',payload:{core_instance_id:'core-a',operation_id:'old',snapshots:[old]}});await flush();assert.equal(s.view.rows[key].snapshot.stats_epoch,'z-new');
  for(const row of g.rows){row.core_instance_id='core-b';row.stats_epoch='new-instance';}g.push();await flush();assert.equal(s.view.rows[key].snapshot.core_instance_id,'core-b');
  g.event({type:'reset',instance:'core-a',payload:{core_instance_id:'core-a',operation_id:'old',snapshots:[old]}});assert.equal(s.view.rows[key].snapshot.core_instance_id,'core-b');
 }finally{s.dispose();}
});
test('event gaps and reconnect establish fresh baselines; query-only operation polls bounded pages',async()=>{
 let now=10000;const g=new FixtureGateway(),s=await started(g,()=>now);try{
  const key=scopeKey(scopes[0]);g.advance();g.push();assert.equal(s.view.rows[key].rates['flow.bytes_down'],1000);
  g.status('gap');g.advance();await flush();assert.equal(s.view.rows[key].rates['flow.bytes_down'],null);
  g.status('subscribed');await flush();assert(s.view.streaming);
 }finally{s.dispose();}
 const queryOnly=new FixtureGateway();queryOnly.caps.capabilities.trafficStatistics.automatic_sampling=false;const q=await started(queryOnly,()=>now);try{assert(!q.view.streaming);const count=queryOnly.queries.length;now+=3001;q.tick();await flush();assert(queryOnly.queries.length>count);}finally{q.dispose();}
});
test('known kernel refusals recover without reset replay; Admin denial closes button',async()=>{
 for(const mode of ['permission_denied','conflict','not_found','invalid_argument','unsupported']){
  const g=new FixtureGateway(),s=await started(g);try{g.mode=mode;await s.reset(s.plan([scopeKey(scopes[3])]));assert.equal(g.commands.length,1);if(mode==='permission_denied')assert.throws(()=>s.plan([scopeKey(scopes[3])]),/管理员/);}finally{s.dispose();}
 }
});
test('history scope limit and dispose prevent late completion and clean only local listeners',async()=>{
 const g=new FixtureGateway();g.rows=Array.from({length:90},(_,i)=>sample({kind:'outbound',tag:`out-${i}`},i));const s=await started(g);s.watch(g.rows.map(r=>scopeKey(r.scope)));g.advance();g.push();assert.equal(Object.values(s.view.rows).filter(r=>r.points.length).length,MAX_HISTORY_SCOPES);
 const view=s.view;s.dispose();g.advance();g.push();assert.equal(s.view,view);assert.equal(g.stops,1);
 const old=new FixtureGateway();old.caps.supported=false;const fallback=await started(old);assert.equal(old.queries.length,0);fallback.dispose();
});

test('switching watched scopes retains recent curves, prioritizes active scopes and evicts oldest histories',async()=>{
 const g=new FixtureGateway();g.rows=Array.from({length:MAX_HISTORY_SCOPES+1},(_,i)=>sample({kind:'outbound',tag:`out-${i}`},i));const s=await started(g);
 try {
  const keys=g.rows.map(row=>scopeKey(row.scope));
  s.watch([keys[0]]);g.advance();g.push();const previous=s.view.rows[keys[0]].points;
  s.watch([keys[1]]);assert.deepEqual(s.view.rows[keys[0]].points,previous);
  g.advance();g.push();assert(s.view.rows[keys[0]].points.length>previous.length);
  s.watch(keys.slice(1));assert.equal(s.view.rows[keys[0]].points.length,0);
  g.advance();g.push();assert.equal(Object.values(s.view.rows).filter(row=>row.points.length).length,MAX_HISTORY_SCOPES);
  s.watch([keys[1]]);g.advance();g.push();assert.equal(Object.values(s.view.rows).filter(row=>row.points.length).length,MAX_HISTORY_SCOPES);
  g.rows.splice(1,1);g.registry='2';await s.refresh();assert.equal(s.view.rows[keys[1]],undefined);
 } finally {s.dispose();}
});

test('workspace retains sampling across page detach, reconnects once and disposes with the app',async()=>{
 const g=new FixtureGateway(),workspace=new TrafficWorkspace(g);let notifications=0;
 let subscriptions=0;const subscribe=g.subscribe.bind(g);g.subscribe=(...args)=>{subscriptions++;return subscribe(...args);};
 try {
 const detach=workspace.connect(()=>notifications++);await flush();
 const key=scopeKey(scopes[0]);workspace.session.watch([key]);g.advance();g.push();
 const previous=workspace.session.view.rows[key].points;detach();const before=notifications;
 g.advance();g.push();assert.equal(notifications,before);assert(workspace.session.view.rows[key].points.length>previous.length);assert.equal(g.stops,0);
 let restored;const detachAgain=workspace.connect(view=>restored=view);
 assert.equal(restored,workspace.session.view);assert.equal(subscriptions,1);
 const unaffected=restored.rows[key].points;await workspace.session.reset(workspace.session.plan([scopeKey(scopes[3])]));
 assert.deepEqual(workspace.session.view.rows[key].points,unaffected);
 await workspace.session.reset(workspace.session.plan([key]));assert.equal(workspace.session.view.rows[key].points.length,1);
 g.advance();g.push();for(const row of g.rows){row.core_instance_id='core-b';row.stats_epoch='next-instance';}g.advance();g.push();await flush();
 assert.equal(workspace.session.view.rows[key].snapshot.core_instance_id,'core-b');assert.equal(workspace.session.view.rows[key].points.length,1);
 detachAgain();workspace.dispose();assert.equal(g.stops,1);assert.throws(()=>workspace.connect(()=>{}),/关闭/);
 } finally {workspace.dispose();}
});

test('GUI delivery envelope decodes exact stats payload and detects gaps across all event types',async()=>{
 const { TrafficStream }=await import('../src/lib/features/traffic/stream.ts');
 const stream=new TrafficStream(),received=[],statuses=[];
 const g=new FixtureGateway(),body=await g.page({offset:0,limit:64});
 function event(sequence,type='traffic.scopesSampled',generation=1){return {generation,event:{eventType:type,sourceEventType:'stats.scopes_sampled',sequenceExact:String(sequence),coreInstanceId:'core-a',payload:{kind:'trafficObservation',data:body}}};}
 stream.receive(event(90071992547409930n),e=>received.push(e),s=>statuses.push(s));
 stream.receive(event(90071992547409931n,'connection.updated'),e=>received.push(e),s=>statuses.push(s));
 stream.receive(event(90071992547409932n),e=>received.push(e),s=>statuses.push(s));
 assert.equal(received.length,2);assert.equal(statuses.length,0);
 stream.receive(event(90071992547409934n),e=>received.push(e),s=>statuses.push(s));assert.deepEqual(statuses,['gap']);
 stream.receive(event(90071992547409934n),e=>received.push(e),s=>statuses.push(s));assert.equal(received.length,3);
 stream.receive(event(1,'traffic.scopesSampled',2),e=>received.push(e),s=>statuses.push(s));
 stream.receive(event(90071992547409935n,'traffic.scopesSampled',1),e=>received.push(e),s=>statuses.push(s));assert.equal(received.length,4);
 const bad=event(2,'traffic.scopesSampled',2);bad.event.payload.kind='wrong';stream.receive(bad,e=>received.push(e),s=>statuses.push(s));assert.equal(statuses.length,3);
});

test('subscribed resync status after native broadcast lag resets differential baseline even while live',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{const key=scopeKey(scopes[0]);g.advance();g.push();assert.equal(s.view.rows[key].rates['flow.bytes_down'],1000);g.advance();g.status('subscribed');await flush();assert.equal(s.view.rows[key].rates['flow.bytes_down'],null);}finally{s.dispose();}
});

test('stale instance, epoch and generation plans are CAS refusals, never silently refreshed reset plans',async()=>{
 for(const changed of ['core_instance_id','stats_epoch','generation']){
  const g=new FixtureGateway(),s=await started(g);try{
   const key=scopeKey(scopes[3]),plan=s.plan([key]);
   if(changed==='core_instance_id')for(const row of g.rows)row.core_instance_id='core-b';else g.rows[3][changed]='new';
   await s.reset(plan);assert.equal(g.commands.length,1);assert.deepEqual(g.commands[0],plan);assert.match(s.view.result,/不会自动重试/);
  }finally{s.dispose();}
 }
});
test('sampling timeout scales with full round-robin cycle, not a fixed one-second cadence',async()=>{
 let now=10000;const g=new FixtureGateway();g.rows=Array.from({length:200},(_,i)=>sample({kind:'outbound',tag:`out-${i}`},i));const s=await started(g,()=>now);try{
  now+=11000;const count=g.queries.length;s.tick();await flush();assert.equal(g.queries.length,count);assert(!s.view.stale);
  now+=1500;s.tick();await flush();assert(g.queries.length>count);
 }finally{s.dispose();}
});

test('failed polling is rate-limited and cannot erase the last complete inventory',async()=>{
 let now=10000;const g=new FixtureGateway();g.caps.capabilities.trafficStatistics.automatic_sampling=false;const s=await started(g,()=>now);try{
  let attempts=0;g.page=async()=>{attempts++;throw{code:'offline'};};now+=3000;s.tick();await flush();assert.equal(attempts,1);assert.equal(s.view.order.length,5);now+=1000;s.tick();await flush();assert.equal(attempts,1);now+=2000;s.tick();await flush();assert.equal(attempts,2);
 }finally{s.dispose();}
});

test('events arriving after failed resync remain buffered until a fresh query establishes the baseline',async()=>{
 const g=new FixtureGateway(),s=await started(g);try{
  const key=scopeKey(scopes[0]),saved=g.page.bind(g);g.page=async()=>{throw{code:'offline'};};await s.refresh();const previous=s.view.rows[key].snapshot;g.advance([0]);g.push([0]);assert.equal(s.view.rows[key].snapshot,previous);assert(s.view.stale);g.page=saved;await s.refresh();assert.equal(s.view.rows[key].snapshot.planes[0].counters.bytes_down,'12000');assert.equal(s.view.rows[key].rates['flow.bytes_down'],null);assert(!s.view.stale);
 }finally{s.dispose();}
});

test('initial offline capability discovery retries and restores real subscription mode',async()=>{
 let now=10000;const g=new FixtureGateway(),discover=g.discover.bind(g);g.discover=async()=>{throw{code:'offline'};};const s=await started(g,()=>now);try{
  assert.equal(s.view.discovery,null);g.discover=discover;now+=3001;s.tick();await flush();assert.equal(s.view.order.length,5);assert(s.view.streaming);
  g.status('gap');await flush();assert(s.view.streaming);
 }finally{s.dispose();}
});
test('upgrade from old capability and downgrade on unsupported query are rediscovered without undeclared traffic polling',async()=>{
 let now=10000;const g=new FixtureGateway();g.caps.supported=false;const s=await started(g,()=>now);try{
  assert.equal(g.queries.length,0);g.caps.supported=true;now+=30001;s.tick();await flush();assert.equal(s.view.order.length,5);assert(s.view.streaming);
  g.caps.supported=false;g.page=async()=>{throw{code:'unsupported'};};now+=30001;s.tick();await flush();assert.equal(s.view.order.length,0);assert(!supported(s.view.discovery));assert(!s.view.streaming);
 }finally{s.dispose();}
});

test('misadvertised unsupported query cannot create an immediate rediscovery loop',async()=>{
 const g=new FixtureGateway();let queries=0;g.page=async()=>{queries++;throw{code:'unsupported'};};const s=await started(g);try{assert(queries<=2);assert.equal(s.view.discovery,null);assert(s.view.stale);}finally{s.dispose();}
});
