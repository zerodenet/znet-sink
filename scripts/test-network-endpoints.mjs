import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EndpointSession } from '../src/lib/features/endpoints/session.ts';
import { operationReason, catalogSupported, directionRequiresRestart } from '../src/lib/features/endpoints/policy.ts';
const caps = { available:true, features:['network_endpoint_catalog_v1','network_endpoint_control_v1','network_endpoint_control_preconditions_v1'], buildFeatures:[],globalLimitations:[],contracts:{capabilities:{current:1,minimumSupported:1},controlApi:{current:1,minimumSupported:1}} };
const endpoint = () => ({ endpoint_id:'opaque:/a',tag:'a',protocol:'future',core_instance_id:'core-1',intent_revision:7,config_revision:1,generation:1,enabled:true,allowed:{inbound:true,outbound:true},effective:{inbound:true,outbound:true},state:'running',state_source:'config',supported:{operations:['set_state','set_directions','restart','details','clear_overrides'],directions:{inbound:true,outbound:true}},counters:{inner_rx_bytes:null,active_stream_flows:0} });
const catalog = () => ({profileId:'profile-a',editableEndpointIds:['opaque:/a'],localOverrideIds:[],capabilities:structuredClone(caps),endpoints:[endpoint()]});
const deferred = () => { let resolve; let reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise,resolve,reject}; };
const gateway = () => ({ catalog:async () => catalog(), control:async () => endpoint(), details:async () => ({endpoint_id:'opaque:/a',generation:1,schema_id:'future',schema_version:1,details:{}}) });
const stop = {operation:'set_state',enabled:false};

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
