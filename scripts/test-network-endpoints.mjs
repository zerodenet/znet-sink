import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EndpointSession } from '../src/lib/features/endpoints/session.ts';
import { operationReason, catalogSupported, directionRequiresRestart } from '../src/lib/features/endpoints/policy.ts';
import { endpointObservation } from '../src/lib/features/endpoints/observation.ts';
import { scopeKey } from '../src/lib/features/traffic/types.ts';
import { observe } from '../src/lib/features/traffic/history.ts';
import { sample } from '../tests/fixtures/traffic.ts';
import { formatEndpointRate } from '../src/lib/features/endpoints/traffic.ts';
const caps = { available:true, features:['network_endpoint_catalog_v1','network_endpoint_control_v1','network_endpoint_control_preconditions_v1'], buildFeatures:[],globalLimitations:[],contracts:{capabilities:{current:1,minimumSupported:1},controlApi:{current:1,minimumSupported:1}} };
const endpoint = () => ({ endpoint_id:'opaque:/a',tag:'a',protocol:'future',core_instance_id:'core-1',intent_revision:7,config_revision:1,generation:1,enabled:true,allowed:{inbound:true,outbound:true},effective:{inbound:true,outbound:true},state:'running',state_source:'config',supported:{operations:['set_state','set_directions','restart','details','clear_overrides'],directions:{inbound:true,outbound:true}},counters:{inner_rx_bytes:null,active_stream_flows:0} });
const catalog = () => ({profileId:'profile-a',editableEndpointIds:['opaque:/a'],localOverrideIds:[],capabilities:structuredClone(caps),endpoints:[endpoint()]});
const deferred = () => { let resolve; let reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise,resolve,reject}; };
const gateway = () => ({ catalog:async () => catalog(), control:async () => endpoint(), details:async () => ({endpoint_id:'opaque:/a',generation:1,schema_id:'future',schema_version:1,details:{}}) });
const stop = {operation:'set_state',enabled:false};

test('scope invalidation retains last-known cards but blocks stale controls until authoritative recovery',async()=>{
  const g=gateway();let controls=0;g.control=async()=>{controls++;return endpoint();};
  const session=new EndpointSession(g,()=>{});await session.refresh();
  const previous=session.view.catalog;
  session.invalidate();
  assert.equal(session.view.catalog,previous);assert.equal(session.view.stale,true);
  await session.act(endpoint(),stop);assert.equal(controls,0);
  const next=deferred();g.catalog=()=>next.promise;
  const refresh=session.refresh();
  assert.equal(session.view.catalog,previous);
  const updated=catalog();updated.endpoints[0].core_instance_id='core-2';
  next.resolve(updated);await refresh;
  assert.equal(session.view.catalog.endpoints[0].core_instance_id,'core-2');
  assert.equal(session.view.stale,false);
  await session.act(endpoint(),stop);assert.equal(controls,0);
  session.dispose();
});

test('legacy endpoint rates distinguish unavailable, idle and positive sub-byte measurements',()=>{
  assert.equal(formatEndpointRate(null),'—');assert.equal(formatEndpointRate(NaN),'—');
  assert.equal(formatEndpointRate(0),'0 B/s');assert.equal(formatEndpointRate(.3),'<1 B/s');
});

test('legacy endpoint roles use declared single role tags without decoding opaque IDs or changing planes',()=>{
 const e={...endpoint(),configuration:{origin:'legacy'},inbound_tags:[],outbound_tags:['declared-role']};
 const snapshot=sample({kind:'outbound',tag:'declared-role'});snapshot.core_instance_id=e.core_instance_id;snapshot.config_revision=String(e.config_revision);
 snapshot.planes[1].counters.rx_bytes=null;snapshot.planes[1].available_metrics=[];
 const key=scopeKey(snapshot.scope),row=observe(undefined,snapshot,true),rows={[key]:row};
 assert.equal(endpointObservation(e,rows).observation,row);assert.equal(endpointObservation(e,rows).key,key);
 assert.equal(endpointObservation(e,rows).observation.snapshot.planes[1].counters.rx_bytes,null);
 for(const next of [{...e,configuration:{origin:'canonical'}},{...e,configuration:undefined},{...e,outbound_tags:['declared-role','other']},{...e,inbound_tags:['also-inbound']},{...e,core_instance_id:'other'},{...e,config_revision:2}]) assert.equal(endpointObservation(next,rows).observation,undefined);
 const inbound={...e,inbound_tags:['declared-role'],outbound_tags:[]};snapshot.scope={kind:'inbound',tag:'declared-role'};
 const incoming=observe(undefined,snapshot,true);assert.equal(endpointObservation(inbound,{[scopeKey(snapshot.scope)]:incoming}).observation,incoming);
});

test('real endpoint observations take priority and reject mismatched instance revision or generation',()=>{
 const e={...endpoint(),configuration:{origin:'legacy'},inbound_tags:[],outbound_tags:['a']};
 const snapshot=sample({kind:'endpoint',endpoint_id:e.endpoint_id});snapshot.core_instance_id=e.core_instance_id;snapshot.config_revision=String(e.config_revision);snapshot.generation=String(e.generation);
 const key=scopeKey(snapshot.scope),row=observe(undefined,snapshot,true),role=sample({kind:'outbound',tag:'a'});
 const rows={[key]:row,[scopeKey(role.scope)]:observe(undefined,role,true)};
 assert.equal(endpointObservation(e,rows).observation,row);
 for(const next of [{...e,core_instance_id:'other'},{...e,config_revision:2},{...e,generation:2}]) assert.equal(endpointObservation(next,rows).observation,undefined);
});

test('compatibility is capability based; direction contraction follows kernel limitations', () => {
  const c = catalog(); const row = endpoint();
  assert.equal(catalogSupported(c), true);
  c.capabilities.features=[]; assert.equal(catalogSupported(c), false);
  assert.match(operationReason(c,row,stop), /未声明/);
  c.capabilities=structuredClone(caps);
  c.capabilities.globalLimitations=['endpoint_live_direction_contraction_requires_stop'];
  const shrink = {operation:'set_directions', directions:{inbound:false,outbound:true}};
  assert.equal(directionRequiresRestart(c,row,shrink.directions),true); assert.equal(operationReason(c,row,shrink),null);
  c.capabilities.globalLimitations=['endpoint_live_outbound_direction_contraction_requires_stop'];
  assert.equal(operationReason(c,row,shrink), null);
  shrink.directions={inbound:true,outbound:false}; assert.equal(directionRequiresRestart(c,row,shrink.directions),true); assert.equal(operationReason(c,row,shrink),null);
  c.capabilities.features=c.capabilities.features.filter(feature=>feature!=='network_endpoint_control_preconditions_v1');
  assert.match(operationReason(c,row,shrink),/升级内核/);
  row.state='stopped'; assert.equal(operationReason(c,row,shrink), null);
  row.supported.directions.inbound=false; assert.match(operationReason(c,row,shrink), /没有配置/);
});

test('no optimistic toggle; exact instance and intent are sent, polling skips pending mutation', async () => {
  const pending=deferred(); let sent; let queries=0;
  const g=gateway(); g.catalog=async () => { queries++; return catalog(); }; g.control=async input => { sent=input; return pending.promise; };
  const session=new EndpointSession(g,()=>{}); await session.refresh();
  const action=session.act(endpoint(),stop);
  assert.equal(session.view.catalog.endpoints[0].enabled,true);
  assert.deepEqual(sent,{profileId:'profile-a',configRevision:1,endpointId:'opaque:/a',coreInstanceId:'core-1',expectedIntentRevision:7,action:stop});
  await session.refresh(); assert.equal(queries,1);
  pending.resolve({...endpoint(),enabled:false,state:'stopped',intent_revision:8}); await action;
  assert.match(session.view.result,/确认操作完成/); assert.equal(queries,2);
});

test('failure remains visible after outcome refresh and is never automatically retried', async () => {
  let calls=0; const g=gateway(); g.control=async()=>{ calls++; throw {code:'conflict',message:'revision changed'}; };
  const session=new EndpointSession(g,()=>{}); await session.refresh(); await session.act(endpoint(),stop);
  assert.equal(calls,1); assert.equal(session.view.error.code,'conflict'); assert.equal(session.view.busy,null);
  assert.equal(session.view.result,null); await session.refresh(false); assert.equal(session.view.error.code,'conflict');
});

test('scope changes discard late query and control publication', async () => {
  const pending=deferred(); const g=gateway(); g.catalog=()=>pending.promise;
  const session=new EndpointSession(g,()=>{}); const refresh=session.refresh(); session.invalidate(); pending.resolve(catalog()); await refresh;
  assert.equal(session.view.catalog,null);
  g.catalog=async()=>catalog(); await session.refresh(); const actionResult=deferred(); g.control=()=>actionResult.promise;
  const action=session.act(endpoint(),stop); session.invalidate();
  const other=catalog();other.endpoints[0].core_instance_id='core-2';g.catalog=async()=>other;
  actionResult.resolve({...endpoint(),enabled:false}); await action;
  assert.equal(session.view.result,null); assert.equal(session.view.catalog.endpoints[0].core_instance_id,'core-2');
});

test('late details cannot replace a newer instance; unknown counters stay null', async () => {
  const g=gateway(); const session=new EndpointSession(g,()=>{}); await session.refresh();
  g.details=async()=>({endpoint_id:'opaque:/a',generation:2}); await session.inspect(endpoint());
  assert.equal(session.view.details,null); assert.match(session.view.detailsError.message,/实例已变化/);
  assert.equal(session.view.catalog.endpoints[0].counters.inner_rx_bytes,null);
  assert.equal(session.view.catalog.endpoints[0].counters.active_stream_flows,0);
  session.dispose(); await session.refresh();
});

test('uncertain mutation blocks controls until reconciliation read completes, preserving both errors', async () => {
  const g=gateway(); let actions=0;
  g.control=async()=>{ actions++; throw {code:'timeout',message:'outcome unknown'}; };
  const session=new EndpointSession(g,()=>{}); await session.refresh();
  const read=deferred();g.catalog=()=>read.promise;
  const first=session.act(endpoint(),stop); await new Promise(resolve=>setImmediate(resolve));
  assert.equal(session.view.busy,null); assert.equal(session.view.stale,true); assert.equal(session.view.loading,true);
  await session.act(endpoint(),stop); assert.equal(actions,1);
  read.reject({message:'refresh unavailable'});await first;
  assert.equal(session.view.error.code,'timeout');assert.equal(session.view.refreshError.message,'refresh unavailable');
  g.catalog=async()=>catalog();await session.refresh(false);
  assert.equal(session.view.stale,false);assert.equal(session.view.refreshError,null);assert.equal(session.view.error.code,'timeout');
});

test('opened protocol facts update on polling and disappear on failed observation', async () => {
  const g=gateway();let reads=0;
  g.details=async()=>({endpoint_id:'opaque:/a',generation:1,schema_id:'future',schema_version:1,details:{sample:++reads}});
  const session=new EndpointSession(g,()=>{});await session.refresh();await session.inspect(endpoint());
  assert.equal(session.view.details.details.sample,1);
  await session.refresh(false);assert.equal(session.view.details.details.sample,2);
  g.details=async()=>{throw {message:'observer failed'};};await session.refresh(false);
  assert.equal(session.view.details,null);assert.equal(session.view.detailsError.message,'observer failed');
  assert.equal(session.view.stale,false);
});

test('background observation keeps catalog and compatible details visible without foreground loading', async () => {
  const g = gateway(); const publications = [];
  const session = new EndpointSession(g, view => publications.push(view));
  await session.refresh(); await session.inspect(endpoint());
  const previousCatalog = session.view.catalog;
  const previousDetails = session.view.details;
  publications.length = 0;
  const directory = deferred(); const detail = deferred(); let reads = 0;
  g.catalog = () => { reads++; return directory.promise; };
  g.details = () => detail.promise;
  const refresh = session.refresh(false, true);
  await session.refresh(false, true);
  assert.equal(reads, 1);
  assert.equal(session.view.catalog, previousCatalog);
  assert.equal(session.view.details, previousDetails);
  directory.resolve(catalog()); await new Promise(resolve => setImmediate(resolve));
  assert.equal(session.view.details, previousDetails);
  detail.resolve({...previousDetails, details:{sample:'new'}}); await refresh;
  assert.equal(session.view.details.details.sample, 'new');
  assert.ok(publications.every(view => !view.loading && !view.detailsLoading && view.catalog && view.details));
});

test('background reads are superseded by control and do not block its authoritative readback', async () => {
  const g = gateway(); const session = new EndpointSession(g, () => {});
  await session.refresh();
  const old = deferred(); g.catalog = () => old.promise;
  const refresh = session.refresh(false, true);
  const updated = {...endpoint(),enabled:false,state:'stopped',intent_revision:8};
  g.control = async () => updated;
  g.catalog = async () => ({...catalog(),endpoints:[updated]});
  await session.act(endpoint(),stop);
  assert.equal(session.view.stale,false);
  assert.equal(session.view.catalog.endpoints[0].enabled,false);
  old.resolve(catalog()); await refresh;
  assert.equal(session.view.catalog.endpoints[0].enabled,false);
});

test('background observation hides incompatible generation details and exposes failures', async () => {
  const g = gateway(); const session = new EndpointSession(g, () => {});
  await session.refresh(); await session.inspect(endpoint());
  const current = {...endpoint(),generation:2};
  g.catalog = async () => ({...catalog(),endpoints:[current]});
  const detail = deferred(); g.details = () => detail.promise;
  const refresh = session.refresh(false,true);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(session.view.details,null);
  detail.resolve({endpoint_id:current.endpoint_id,generation:2,details:{}}); await refresh;
  g.catalog = async () => { throw {message:'offline'}; };
  await session.refresh(false,true);
  assert.equal(session.view.stale,true);
  assert.equal(session.view.refreshError.message,'offline');
  assert.equal(session.view.details,null);
  assert.equal(session.view.catalog.endpoints[0].generation,2);
});

test('stale row actions are refused locally and disposal stops details publication', async () => {
  const g=gateway();let calls=0;g.control=async()=>{calls++;return endpoint();};
  const session=new EndpointSession(g,()=>{});await session.refresh();
  await session.act({...endpoint(),intent_revision:6},stop);assert.equal(calls,0);
  const pending=deferred();g.details=()=>pending.promise;
  const inspect=session.inspect(endpoint());session.dispose();pending.resolve({endpoint_id:'opaque:/a',generation:1});await inspect;
  assert.equal(session.view.details,null);
});

// Endpoint counters are cumulative. Only fresh samples in the same instance
// may produce a rate; resets and unknown counters must break the graph.
test('traffic deltas use observation time and independent receive/send counters', async () => {
  const { sampleEndpointTraffic, trafficPaths } = await import('../src/lib/features/endpoints/traffic.ts');
  const row = {...endpoint(),observed_at_unix_ms:1000,counters:{inner_rx_bytes:1000,inner_tx_bytes:2000}};
  const first = sampleEndpointTraffic(undefined,row);
  assert.equal(first.points[0].rx,null);
  const second = sampleEndpointTraffic(first,{...row,observed_at_unix_ms:6000,counters:{inner_rx_bytes:11240,inner_tx_bytes:7120}});
  assert.equal(second.points[1].rx,2048);
  assert.equal(second.points[1].tx,1024);
  assert.equal(sampleEndpointTraffic(second,{...row,observed_at_unix_ms:5000}),second);
  assert.equal(sampleEndpointTraffic(second,{...row,observed_at_unix_ms:6000}),second);
  assert.match(trafficPaths(second.points,'rx',4096),/^ M/);
});

test('traffic resets, counter gaps and long pauses never fabricate a continuous rate', async () => {
  const { sampleEndpointTraffic, trafficPaths } = await import('../src/lib/features/endpoints/traffic.ts');
  const row = {...endpoint(),observed_at_unix_ms:1000,counters:{inner_rx_bytes:10000,inner_tx_bytes:null}};
  let history = sampleEndpointTraffic(undefined,row);
  history = sampleEndpointTraffic(history,{...row,observed_at_unix_ms:6000,counters:{inner_rx_bytes:11000,inner_tx_bytes:null}});
  assert.equal(history.points.at(-1).rx,200);
  assert.equal(history.points.at(-1).tx,null);
  history = sampleEndpointTraffic(history,{...row,observed_at_unix_ms:11000,counters:{inner_rx_bytes:10,inner_tx_bytes:null}});
  assert.equal(history.points.at(-1).rx,null);
  history = sampleEndpointTraffic(history,{...row,observed_at_unix_ms:31000,counters:{inner_rx_bytes:2000,inner_tx_bytes:null}});
  assert.equal(history.points.at(-1).rx,null);
  history = sampleEndpointTraffic(history,{...row,observed_at_unix_ms:36000,counters:{inner_rx_bytes:3000,inner_tx_bytes:null}});
  assert.equal(history.points.at(-1).rx,200);
  assert.equal((trafficPaths(history.points,'rx',1024).match(/M/g)??[]).length,2);
  const changed = sampleEndpointTraffic(history,{...row,generation:2,observed_at_unix_ms:41000});
  assert.equal(changed.points.length,1);
  assert.equal(changed.points[0].rx,null);
  const missing = sampleEndpointTraffic(changed,{...row,generation:2,observed_at_unix_ms:46000,counters:{inner_rx_bytes:null,inner_tx_bytes:null}});
  assert.equal(missing.source,null);
  assert.equal(missing.points[0].rx,null);
  const outer = sampleEndpointTraffic(missing,{...row,observed_at_unix_ms:51000,counters:{outer_rx_bytes:100,outer_tx_bytes:200}});
  assert.equal(outer.source,'outer');
  assert.equal(outer.points[0].rx,null);
  const bounded = sampleEndpointTraffic(history,{...row,observed_at_unix_ms:160000});
  assert.ok(bounded.points.every(point => point.at >= 40000));
});

 test('direction submission gates repeated clicks synchronously and retains confirmed checkbox state', async () => {
  let calls=0;const g=gateway();const pending=deferred();
  g.control=async()=>{calls++;return pending.promise;};
  const session=new EndpointSession(g,()=>{});await session.refresh();
  const action={operation:'set_directions',directions:{inbound:false,outbound:true}};
  const first=session.act(endpoint(),action);await session.act(endpoint(),action);
  assert.equal(calls,1);assert.equal(session.view.busy,'opaque:/a');
  assert.equal(session.view.catalog.endpoints[0].allowed.inbound,true);
  pending.reject({message:'reconcile failed'});await first;
  assert.equal(session.view.catalog.endpoints[0].allowed.inbound,true);
  assert.equal(session.view.error.message,'reconcile failed');
});

test('runtime override clearing preserves declared ownership preconditions and refuses client configuration overlays', async()=>{
 const action={operation:'clear_overrides'};
 const c=catalog(),e=endpoint();e.state_source='runtime_override';
 assert.equal(operationReason(c,e,action),null);
 c.localOverrideIds=[e.endpoint_id];assert.match(operationReason(c,e,action),/客户端配置覆盖/);
 c.localOverrideIds=[];c.editableEndpointIds=[];assert.match(operationReason(c,e,action),/仅可观测/);
 c.editableEndpointIds=[e.endpoint_id];c.capabilities.features=c.capabilities.features.filter(feature=>feature!=='network_endpoint_control_preconditions_v1');
 assert.match(operationReason(c,e,action),/前置条件/);
 const g=gateway();const current=catalog();current.endpoints=[e];g.catalog=async()=>current;
 let sent;g.control=async input=>{sent=input;return {...e,state_source:'config'};};
 const session=new EndpointSession(g,()=>{});await session.refresh();await session.act(e,action);
 assert.equal(sent.action.operation,'clear_overrides');assert.equal(sent.expectedIntentRevision,7);assert.equal(sent.coreInstanceId,'core-1');session.dispose();
});

test('endpoint navigation resolves actual registry identity and rejects tag collisions or missing scopes',async()=>{
 const {endpointTrafficTarget,resolveEndpointTraffic}=await import('../src/lib/features/endpoints/traffic-navigation.ts');
 const {discovery}=await import('../tests/fixtures/traffic.ts');
 const e={...endpoint(),tag:'same-tag',inbound_tags:[],outbound_tags:[]};
 const target=endpointTrafficTarget('profile-a',e);
 const actual=sample({kind:'endpoint',endpoint_id:e.endpoint_id});actual.core_instance_id='core-1';actual.config_revision='1';actual.generation='1';
 const unrelated=sample({kind:'outbound',tag:'same-tag'});
 const view={discovery:discovery(),order:[scopeKey(unrelated.scope)],rows:{[scopeKey(unrelated.scope)]:observe(undefined,unrelated,true)},loading:false,stale:false};
 assert.equal(resolveEndpointTraffic(target,view).key,null);
 view.order.push(scopeKey(actual.scope));view.rows[scopeKey(actual.scope)]=observe(undefined,actual,true);
 assert.equal(resolveEndpointTraffic(target,view).key,scopeKey(actual.scope));
 assert.equal(resolveEndpointTraffic(target,view,false).key,null);
});

test('endpoint navigation checks instance revision and generation without truncating traffic u64',async()=>{
 const {endpointTrafficTarget,resolveEndpointTraffic}=await import('../src/lib/features/endpoints/traffic-navigation.ts');
 const {discovery}=await import('../tests/fixtures/traffic.ts');
 const e={...endpoint(),inbound_tags:[],outbound_tags:[]};const target=endpointTrafficTarget('profile-a',e);
 const row=sample({kind:'endpoint',endpoint_id:e.endpoint_id});Object.assign(row,{core_instance_id:e.core_instance_id,config_revision:'1',generation:'1'});
 row.planes[1].counters.rx_bytes='18446744073709551615';
 const key=scopeKey(row.scope);const view={discovery:discovery(),order:[key],rows:{[key]:observe(undefined,row,true)},loading:false,stale:false};
 assert.equal(resolveEndpointTraffic(target,view).key,key);assert.equal(view.rows[key].snapshot.planes[1].counters.rx_bytes,'18446744073709551615');
 for(const patch of [{core_instance_id:'core-2'},{config_revision:'2'},{generation:'2'},{config_revision:'9007199254740993'}]){view.rows[key]=observe(undefined,{...row,...patch},true);assert.equal(resolveEndpointTraffic(target,view).key,null);}
 assert.equal(endpointTrafficTarget('profile-a',{...e,config_revision:Number.MAX_SAFE_INTEGER+1}),null);
 assert.equal(endpointTrafficTarget('profile-a',{...e,generation:Number.MAX_SAFE_INTEGER+1}),null);
});

test('legacy navigation requires real unambiguous declared role and never bypasses mismatched endpoint generation',async()=>{
 const {endpointTrafficTarget,resolveEndpointTraffic,matchesEndpointTrafficTarget}=await import('../src/lib/features/endpoints/traffic-navigation.ts');
 const {discovery}=await import('../tests/fixtures/traffic.ts');
 const e={...endpoint(),configuration:{origin:'legacy'},inbound_tags:[],outbound_tags:['actual-role']};
 const target=endpointTrafficTarget('profile-a',e),role=sample({kind:'outbound',tag:'actual-role'});
 Object.assign(role,{core_instance_id:e.core_instance_id,config_revision:'1'});role.planes=role.planes.filter(p=>p.plane==='inner');
 const key=scopeKey(role.scope),view={discovery:discovery(),order:[key],rows:{[key]:observe(undefined,role,true)},loading:false,stale:false};
 const resolved=resolveEndpointTraffic(target,view);assert.equal(resolved.key,key);assert.equal(resolved.role,true);assert.deepEqual(resolved.planes,['inner']);assert.match(resolved.reason,/角色统计/);
 assert.equal(matchesEndpointTrafficTarget(target,'profile-b',e),false);assert.equal(matchesEndpointTrafficTarget(target,'profile-a',{...e,generation:2}),false);
 assert.equal(resolveEndpointTraffic(endpointTrafficTarget('profile-a',{...e,outbound_tags:['actual-role','other']}),view).key,null);
 assert.equal(resolveEndpointTraffic(endpointTrafficTarget('profile-a',{...e,configuration:{origin:'canonical'}}),view).key,null);
 const wrong=sample({kind:'endpoint',endpoint_id:e.endpoint_id});Object.assign(wrong,{core_instance_id:e.core_instance_id,config_revision:'1',generation:'2'});view.order.push(scopeKey(wrong.scope));view.rows[scopeKey(wrong.scope)]=observe(undefined,wrong,true);
 assert.equal(resolveEndpointTraffic(target,view).key,null);
});

test('endpoint client binding requires positive safe host generation and changes across unobserved ABA',async()=>{
 const {endpointClientBinding}=await import('../src/lib/features/endpoints/traffic-navigation.ts');
 const base={scope:{profileId:'a',configRevision:1,coreInstanceId:1},scopeGeneration:1};
 const original=endpointClientBinding(base);assert.equal(typeof original,'string');
 for(const generation of [undefined,0,-1,1.1,NaN,Infinity,Number.MAX_SAFE_INTEGER+1])assert.equal(endpointClientBinding({...base,scopeGeneration:generation}),undefined);
 assert.notEqual(endpointClientBinding({...base,scopeGeneration:3}),original);
 assert.equal(endpointClientBinding({...base,revision:99}),original);
 assert.match(endpointClientBinding({...base,scope:{...base.scope,configRevision:'18446744073709551615'}}),/18446744073709551615/);
 for(const revision of ['18446744073709551616','01',Number.MAX_SAFE_INTEGER+1]) assert.equal(endpointClientBinding({...base,scope:{...base.scope,configRevision:revision}}),undefined);
});


test('disabled endpoint statistics entry explains stale host binding and unsafe endpoint identity',async()=>{
 const {endpointTrafficUnavailableReason}=await import('../src/lib/features/endpoints/traffic-navigation.ts');
 const e={...endpoint(),inbound_tags:[],outbound_tags:[]};
 assert.match(endpointTrafficUnavailableReason(e,true,'binding'),/目录未确认/);
 assert.match(endpointTrafficUnavailableReason(e,false),/运行代次未确认/);
 assert.match(endpointTrafficUnavailableReason({...e,generation:Number.MAX_SAFE_INTEGER+1},false,'binding'),/安全范围/);
 assert.equal(endpointTrafficUnavailableReason(e,false,'binding'),null);
});
