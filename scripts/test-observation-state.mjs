import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { SourceTextModule, SyntheticModule, createContext } from 'node:vm';
import ts from 'typescript';

const copy = value => JSON.parse(JSON.stringify(value));
const deferred = () => { let resolve; const promise = new Promise(r => {resolve=r;}); return {promise,resolve}; };
const flow = (id='one',revision=1) => ({flowId:id,revision,network:'tcp',destination:'example.test:443',bytesUp:revision,bytesDown:0,selectionChain:[],relayChain:[]});
const snapshot = (id='engine-a',flows=[flow()]) => ({runtime:{core_instance_id:id,config_revision:1},stats:{connections:flows.length},policies:[],connections:{items:flows}});

// Execute actual stores and lifecycle code. Only the Tauri and sibling-feature
// adapters are substituted; no DOM, host network, or installed kernel is used.
async function harness() {
  const state={generation:1,callbacks:new Map(),startGate:null,policyGate:null,readGate:null,reads:0,stats:[],rates:[],runtimes:[],stopCalls:0,startCalls:0,endpoint:'desktop/zero-control.sock',bindings:[],startEntered:deferred()};
  const context=createContext({$state:value=>value,console,Date,Map,Set,setTimeout,clearTimeout});
  const noOp=async()=>{};
  const core={
    startGuiEvents:async()=>{state.startCalls++;state.bindings.push(state.endpoint);state.startEntered.resolve();if(state.startGate)await state.startGate;return {generation:state.generation};},
    stopGuiEvents:async()=>{state.stopCalls++;state.generation++;},appendLog:noOp,
    getGuiObservationSnapshot:async()=>{state.reads++;return state.readGate ? await state.readGate : snapshot();},
  };
  const adapters={
    '@tauri-apps/api/event':{listen:async(name,callback)=>{state.callbacks.set(name,callback);return ()=>state.callbacks.delete(name);}},
    '$lib/services/core':core,
    '$lib/services/overview-data.svelte':{overviewData:{applyStatsEvent:value=>state.stats.push(copy(value)),applyRuntimeEvent:value=>state.runtimes.push(copy(value)),applyPolicyEvent(){},applyTrafficRateSample:value=>state.rates.push(copy(value)),refreshPolicyNodes:noOp}},
    '$lib/services/gui-state.svelte':{guiState:{refreshPolicyGroups:async()=>{if(state.policyGate)await state.policyGate;},refreshNodeStateAfterConfigChange:noOp,applyPolicyProbeCompleted(){},probeNetwork:noOp,refreshSelfTest:noOp}},
    '$lib/services/toast.svelte':{warning(){}},
  };
  const modules=new Map();
  async function load(url) {
    if(modules.has(url))return modules.get(url);
    const source=await readFile(new URL(url),'utf8');
    const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
    const module=new SourceTextModule(code,{context,identifier:url});modules.set(url,module);
    await module.link(async(specifier,reference)=>{
      if(adapters[specifier]) {
        const values=adapters[specifier];
        return new SyntheticModule(Object.keys(values),function(){for(const [key,value] of Object.entries(values))this.setExport(key,value);},{context});
      }
      const target=specifier.startsWith('$lib/') ? new URL(`../src/lib/${specifier.slice(5)}.ts`,import.meta.url) : new URL(`${specifier}.ts`,reference.identifier);
      return load(target.href);
    });
    return module;
  }
  const service=await load(new URL('../src/lib/services/core-events.svelte.ts',import.meta.url).href);
  await service.evaluate();
  const feature=modules.get(new URL('../src/lib/features/observations/connections.svelte.ts',import.meta.url).href).namespace;
  return {state,service:service.namespace.coreEvents,feature,
    status:(status,response,generation=state.generation)=>state.callbacks.get('gui:event-status')?.({payload:{generation,status,response}}),
    event:(eventType,data,generation=state.generation)=>state.callbacks.get('gui:event')?.({payload:{generation,event:{eventType,sourceEventType:eventType,payload:{data}}}}),
  };
}

test('startup buffers events until the returned generation selects the exact stream',async()=>{
  const h=await harness();const gate=deferred();h.state.startGate=gate.promise;
  const starting=h.service.start();await h.state.startEntered.promise;
  h.status('subscribed',snapshot('stale',[flow('stale')]),0);
  h.status('subscribed',snapshot());h.event('connection.updated',flow('one',2));
  assert.equal(h.feature.connectionObservations.activeConnections.length,0);
  gate.resolve();await starting;
  assert.deepEqual(copy(h.feature.connectionObservations.activeConnections).map(f=>[f.flowId,f.revision]),[['one',2]]);
  assert.equal(h.state.runtimes[0].core_instance_id,'engine-a');
});

test('slow policy refresh cannot overwrite newer connection deltas',async()=>{
  const h=await harness();await h.service.start();const gate=deferred();h.state.policyGate=gate.promise;
  h.status('subscribed',snapshot());
  h.event('connection.updated',flow('one',8));
  gate.resolve();await Promise.resolve();await Promise.resolve();
  assert.equal(h.feature.connectionObservations.activeConnections[0].revision,8);
});

test('late snapshot read after stop or reconnect never updates the new scope',async()=>{
  for(const action of ['stop','reconnect']) {
    const h=await harness();await h.service.start();h.status('subscribed',snapshot());
    const gate=deferred();h.state.readGate=gate.promise;
    h.event('core.statusChanged',{healthy:true});assert.equal(h.state.reads,1);
    if(action==='stop')await h.service.stop();else h.status('reconnecting');
    gate.resolve(snapshot('stale'));await Promise.resolve();await Promise.resolve();
    assert.deepEqual(h.state.runtimes.map(r=>r.core_instance_id),['engine-a']);
    assert.equal(h.feature.connectionObservations.activeConnections.length,0);
    assert.equal(h.feature.connectionObservations.drainDeltas().at(-1).type,'snapshot');
  }
});

test('runtime replacement permits a reused flow ID and drops old runtime history',async()=>{
  const h=await harness();await h.service.start();h.status('subscribed',snapshot());
  h.event('connection.closed',flow('one',5));
  assert.equal(h.feature.connectionObservations.connectionHistory.length,1);
  h.status('subscribed',snapshot('engine-b',[flow('one',1)]));
  assert.equal(h.feature.connectionObservations.connectionHistory.length,0);
  h.event('connection.updated',flow('one',2));
  assert.equal(h.feature.connectionObservations.activeConnections[0].revision,2);
});

test('restarted stream rejects the previous generation and releases listeners',async()=>{
  const h=await harness();await h.service.start();h.status('subscribed',snapshot());
  await h.service.stop();assert.equal(h.state.callbacks.size,0);
  h.state.generation=2;await h.service.start();h.status('subscribed',snapshot('engine-b'));
  h.event('connection.started',flow('stale'),1);h.status('offline',undefined,1);
  assert.equal(h.service.status,'subscribed');
  assert.equal(h.feature.connectionObservations.activeConnections.length,1);
});

test('startup overflow is bounded, visible, and recoverable',async()=>{
  const h=await harness();const gate=deferred();h.state.startGate=gate.promise;
  const starting=h.service.start();await h.state.startEntered.promise;
  for(let i=0;i<2001;i++)h.event('connection.updated',flow());
  gate.resolve();await starting;
  assert.equal(h.service.status,'error');assert.match(h.service.lastError,/overflow/);
  assert.equal(h.state.callbacks.size,0);assert.equal(h.state.stopCalls,1);
  h.state.startGate=null;await h.service.start();h.status('subscribed',snapshot());
  assert.equal(h.service.status,'subscribed');
});

test('rejected stale revisions never escape through the consumer delta queue',async()=>{
  const h=await harness();const store=new h.feature.ConnectionObservations();
  store.apply({type:'started',connection:flow('one',4)});store.drainDeltas();
  store.apply({type:'updated',connection:flow('one',2)});
  store.apply({type:'completed',connection:flow('one',3)});
  assert.equal(store.drainDeltas().length,0);assert.equal(store.activeConnections[0].revision,4);
  store.apply({type:'completed',connection:flow('one',5)});store.drainDeltas();
  store.apply({type:'started',connection:flow('one',1)});
  assert.equal(store.activeConnections.length,0);assert.equal(store.drainDeltas().length,0);
});

test('a slow consumer recovers from a complete bounded projection',async()=>{
  const h=await harness();const store=new h.feature.ConnectionObservations();
  for(let i=0;i<2100;i++)store.apply({type:'started',connection:flow(String(i))});
  const deltas=store.drainDeltas();assert.ok(deltas.length<=2000);
  assert.equal(deltas[0].type,'snapshot');assert.equal(deltas[0].connections.length,500);
  assert.equal(store.activeConnections.length,500);
});

test('connection events while reconnecting cannot repopulate the offline projection',async()=>{
  const h=await harness();await h.service.start();h.status('subscribed',snapshot());
  h.status('reconnecting');h.event('connection.updated',flow('stale',10));
  assert.equal(h.feature.connectionObservations.activeConnections.length,0);
});

test('an invalid fallback response does not recursively issue more queries',async()=>{
  const h=await harness();await h.service.start();h.status('subscribed',snapshot());
  h.state.readGate=Promise.resolve(null);h.event('core.statusChanged',{healthy:true});
  await Promise.resolve();await Promise.resolve();await Promise.resolve();
  assert.equal(h.state.reads,1);
});

test('kernel installation, rollback and endpoint edits rebind the stream and restore rates',async()=>{
  for(const reason of ['core.version_restarted','core.version_rollback','core.endpoint_changed']) {
    const h=await harness();await h.service.start();h.status('offline');
    const oldGeneration=h.state.generation;
    h.state.endpoint='installed/zero-control.sock';
    h.state.callbacks.get('host-network:changed')({payload:{reason}});
    // The queued start waits for the notification's stop/start transaction.
    await h.service.start();
    assert.deepEqual(h.state.bindings,['desktop/zero-control.sock','installed/zero-control.sock']);
    assert.equal(h.state.stopCalls,1);
    h.status('offline',undefined,oldGeneration);h.status('subscribed',snapshot('new-kernel'));
    const rate=h.state.callbacks.get('traffic:rate-sampled');
    rate({payload:{generation:oldGeneration,stable:true,downloadBytesPerSec:999999}});
    assert.equal(h.state.rates.length,0);
    rate({payload:{generation:h.state.generation,stable:true,sampledAtUnixMs:Date.now(),downloadBytesPerSec:12000}});
    assert.equal(h.service.status,'subscribed');assert.equal(h.state.rates.at(-1).downloadBytesPerSec,12000);
    assert.equal(h.state.callbacks.size,5);
  }
});

test('ordinary host network changes preserve the stream and late notifications cannot revive a stopped stream',async()=>{
  const h=await harness();await h.service.start();h.status('subscribed',snapshot());
  const callback=h.state.callbacks.get('host-network:changed');
  callback({payload:{reason:'system_proxy.enabled'}});await h.service.start();
  assert.equal(h.state.startCalls,1);assert.equal(h.state.stopCalls,0);
  const stopping=h.service.stop();callback({payload:{reason:'core.version_restarted'}});
  await stopping;await new Promise(resolve=>setTimeout(resolve,0));
  assert.equal(h.state.startCalls,1);assert.equal(h.state.callbacks.size,0);
});
