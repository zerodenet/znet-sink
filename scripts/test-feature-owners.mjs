import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import path from 'node:path';
const cache = new Map();
import { createLatestRequestGate } from '../src/lib/services/latest-request-gate.js';
function load(file) {
 if (cache.has(file)) return cache.get(file);
 const source = readFileSync(new URL(`../src/lib/${file}`, import.meta.url),'utf8');
 const {outputText} = ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
 const exports = {}; cache.set(file, exports);
 new Function('require','exports','$state',outputText)(name => {
   if (name.startsWith('.')) return load(path.posix.normalize(path.posix.join(path.posix.dirname(file), `${name}${name.endsWith('.ts') ? '' : '.ts'}`)));
   if (name === '$lib/services/latest-request-gate.js') return {createLatestRequestGate};
   if (name.startsWith('$lib/')) return load(`${name.slice(5)}${name.endsWith('.js') || name.endsWith('.ts') ? '' : '.ts'}`);
   throw new Error(name);
 }, exports, value=>value);
 return exports;
}
const {PolicyState}=load('features/policies/state.svelte.ts');
const {NetworkProbeState}=load('features/probes/network.svelte.ts');
const {DnsState}=load('features/dns/state.svelte.ts');
const {RouteTraceState}=load('features/routing/state.svelte.ts');
const {collectModules}=load('features/diagnostics/model.ts');
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const ports={getGuiProxyModeStatus:async()=>({}),getConfigProxyNodes:async()=>[],getConfigPolicyGroups:async()=>[],getGuiPolicyGroups:async()=>[],errorMessage:e=>e.message};
test('destroyed policy owner rejects late snapshots and profile reconciliation',async()=>{
 const wait=deferred();let reads=0;
 const state=new PolicyState({...ports,getConfigPolicyGroups:()=>wait.promise,getGuiPolicyGroups:async()=>{reads++;return [];}});
 state.policyGroups=[{name:'old',outbounds:[]}];
 const pending=state.refreshNodeStateAfterConfigChange();state.invalidate();wait.resolve([]);await pending;
 assert.equal(reads,0);assert.equal(state.policyGroups[0].name,'old');
});
test('independent policy owners do not share queries or failed observations',async()=>{
 const wait=deferred();const first=new PolicyState({...ports,getGuiPolicyGroups:()=>wait.promise});const second=new PolicyState(ports);
 const pending=first.refreshPolicyGroups();first.invalidate();await second.refreshPolicyGroups();wait.reject(new Error('old failure'));await pending;
 assert.equal(first.policyGroupsError,null);assert.equal(second.policyGroupsError,null);assert.ok(second.policyGroupsUpdatedAt>0);
});
test('network probe coalesces repeated requests and disposal prevents a queued rerun',async()=>{
 const wait=deferred();let calls=0,changed=0;
 const probe=new NetworkProbeState({query:()=>{calls++;return wait.promise;},changed:()=>changed++,error:e=>e.message});
 const pending=probe.run();await probe.run();await probe.run();probe.stop();wait.resolve({});await pending;
 assert.equal(calls,1);assert.equal(changed,0);assert.equal(probe.loading,false);assert.equal(probe.result,null);
});
test('network probe performs one follow-up for a burst',async()=>{
 const first=deferred();let calls=0;
 const probe=new NetworkProbeState({query:()=>++calls===1?first.promise:Promise.resolve({}),changed:()=>{},error:e=>e.message});
 const pending=probe.run();void probe.run();void probe.run();first.resolve({});await pending;await Promise.resolve();
 assert.equal(calls,2);probe.stop();
});
test('disposed DNS and route tools never publish delayed results',async()=>{
 const wait=deferred();const dns=new DnsState({guiDnsLookup:()=>wait.promise,getAppErrorMessage:e=>String(e)});
 const route=new RouteTraceState({guiTraceRoute:()=>wait.promise,getAppErrorMessage:e=>String(e)});
 dns.dnsHost='example.test';route.traceTarget='example.test';
 const tasks=[dns.runDns(),route.runTrace()];dns.dispose();route.dispose();wait.resolve({});await Promise.all(tasks);
 assert.equal(dns.dnsResult,null);assert.equal(route.traceResult,null);assert.equal(dns.dnsLoading,false);
});
test('Fake-IP cleanup and lookup cannot overlap or replay on cache read failure',async()=>{
 const wait=deferred();let mutations=0,queries=0;
 const dns=new DnsState({guiClearFakeIp:()=>{mutations++;return wait.promise;},guiFakeIpLookup:async()=>{queries++;return {};},guiDnsCache:async()=>{throw new Error('cache unavailable');},getAppErrorMessage:e=>e.message,confirm:()=>true});
 dns.fakeQuery='example.test';const pending=dns.clearFakeIp('selected');await dns.runFakeIp();await dns.clearFakeIp('selected');
 wait.resolve({enabled:true,removedMappings:1,removedAddresses:1,liveMappings:0,retiredAddresses:0});await pending;
 assert.equal(mutations,1);assert.equal(queries,0);assert.equal(dns.fakeError,'cache unavailable');
});
test('module diagnostics keeps other sources when one fails or times out',async()=>{
 const local={id:'local',title:'local',state:'idle',summary:'idle',facts:[]};
 const report=await collectModules([{id:'host',title:'host',read:()=>new Promise(()=>{})},{id:'config',title:'config',read:async()=>local}], [local], 10);
 assert.equal(report.modules[0].state,'unavailable');assert.match(report.modules[0].error,/超时/);assert.equal(report.modules[1],local);assert.equal(report.modules[2],local);
});
test('probe job owner releases registrations that complete after disposal',async()=>{
 const {ProbeJobsState}=load('features/node-probes/jobs.svelte.ts');const query=deferred(),registration=deferred();let stopped=0;
 const owner=new ProbeJobsState(()=>query.promise);const pending=owner.refresh('test');const attaching=owner.attach(registration.promise);
 owner.dispose();registration.resolve(()=>stopped++);query.resolve({revision:1});await Promise.all([pending,attaching]);
 assert.equal(stopped,1);assert.equal(owner.nodeScreen,null);
});

test('old diagnostic owner cannot unregister its replacement',()=>{
 const {registerModule,readRegisteredModules}=load('features/diagnostics/registry.ts');
 const status=(summary)=>({id:'test-owner',title:'test',state:'ready',summary,facts:[]});
 const releaseOld=registerModule('test-owner',()=>status('old'));
 const releaseNew=registerModule('test-owner',()=>status('new'));
 releaseOld();assert.equal(readRegisteredModules().find(s=>s.id==='test-owner').summary,'new');
 releaseNew();assert.equal(readRegisteredModules().find(s=>s.id==='test-owner').state,'idle');
});
