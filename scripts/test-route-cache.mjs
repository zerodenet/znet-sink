import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const source = readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8');
  const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } });
  const exports = {}; cache.set(file, exports);
  new Function('require', 'exports', '$state', outputText)(name => {
    const target = name.startsWith('$lib/') ? name.slice(5) : path.posix.normalize(path.posix.join(path.posix.dirname(file), name));
    return load(`${target}${target.endsWith('.ts') ? '' : '.ts'}`);
  }, exports, value => value);
  return exports;
}
const { DnsState } = load('features/dns/state.svelte.ts');
const { RouteTraceState } = load('features/routing/state.svelte.ts');
const { ToolJobsState } = load('features/tool-jobs/state.svelte.ts');
const scope = (profileId = 'one', configRevision = '18446744073709551614', coreInstanceId = 4) => ({ profileId, configRevision, coreInstanceId });
const snapshot = (value = scope(), revision = 1, sourceStatus = 'ready') => ({ revision, scopeGeneration: revision, scope: value, sourceStatus, activeProbeJobs: [] });
const job = (id, kind, state, value = scope(), result, updatedAtUnixMs = 1) => ({ id, kind, state, scope: value, subject: 'same.example', params: {}, result, createdAtUnixMs: 1, updatedAtUnixMs, deadlineAtUnixMs: 1000 });
const flush = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
function fixture(history = [], initial = snapshot()) {
  let current = initial, listener = () => {}, scopeListener = () => {}, id = 100;
  const calls = [], cancellations = [];
  const ports = {
    start: async request => { calls.push(request); return job(++id, request.kind, 'queued', request.expectedScope); },
    list: async () => history,
    cancel: async (id, expected) => { cancellations.push({ id, expected }); return job(id, 'dns_lookup', 'cancelled', expected, undefined, 8); },
    subscribe: async handler => { listener = handler; return () => {}; },
    scope: async () => current,
    subscribeScope: async handler => { scopeListener = handler; return () => {}; },
    getAppErrorMessage: error => error.message,
    confirm: () => true,
  };
  return { ports, calls, cancellations, get current() { return current; }, set(value) { current = value; scopeListener(value); }, emit(value) { listener(value); } };
}

const {parseRouteEvaluation}=load('features/routing/result.ts');
const {parseDnsCache}=load('features/dns/cache-result.ts');
const routeResult=()=>({target:'example.test',port:443,protocol:'tcp',inbound_tag:null,effective_mode:'rule',route_action:{route:'proxy'},matched_rule:{index:'18446744073709551615',condition:'domain example.test'}});
test('actual route facts preserve the complete rule index and expose no guessed hops or leaf',()=>{
 const result=parseRouteEvaluation({...routeResult(),hops:[{rtt:12}],secret:'never-copy'});
 assert.equal(result.matched_rule.index,'18446744073709551615');assert.deepEqual(result.route_action,{route:'proxy'});assert.equal(result.hops,undefined);assert.equal(result.secret,undefined);
 for(const route_action of ['direct','reject'])assert.equal(parseRouteEvaluation({...routeResult(),route_action,matched_rule:null}).route_action,route_action);
});
test('missing, unknown or unsafe route facts are incompatible rather than direct defaults',()=>{
 for(const key of ['target','port','protocol','inbound_tag','effective_mode','route_action','matched_rule']){const value=routeResult();delete value[key];assert.throws(()=>parseRouteEvaluation(value));}
 for(const matched_rule of [{index:9007199254740992,condition:'x'},{index:'18446744073709551616',condition:'x'}])assert.throws(()=>parseRouteEvaluation({...routeResult(),matched_rule}));
 assert.throws(()=>parseRouteEvaluation({accepted:true,result:routeResult()}));assert.throws(()=>parseRouteEvaluation({hops:[]}));
});
test('DNS list, miss, disabled and zero TTL remain distinct, without missing count coercion',()=>{
 const list=parseDnsCache({enabled:true,entries:[{domain:'a.test',addresses:['192.0.2.1'],ttl_seconds:'18446744073709551615'}],count:1});assert.equal(list.entries[0].ttl_seconds,'18446744073709551615');
 assert.equal(parseDnsCache({enabled:false,entries:[],count:0}).enabled,false);
 assert.equal(parseDnsCache({enabled:true,domain:'a.test',hit:false,addresses:[],ttl_seconds:null}).hit,false);
 assert.equal(parseDnsCache({enabled:true,domain:'a.test',hit:true,addresses:['192.0.2.1'],ttl_seconds:'0'}).ttl_seconds,'0');
 for(const value of [{},{enabled:true},{enabled:true,entries:[]},{enabled:true,entries:[],count:1},{enabled:true,domain:'a.test',hit:true,addresses:[],ttl_seconds:null},{enabled:true,entries:[{domain:'a.test',addresses:[],ttl_seconds:9007199254740992}],count:1}])assert.throws(()=>parseDnsCache(value));
});
test('standalone DNS cache fixes domain and limit in the existing scope without FakeIP or lookup',async()=>{
 const f=fixture(),dns=new DnsState(f.ports);await dns.jobs.init();dns.cacheDomain='  a.test ';dns.cacheLimit=20;await dns.runCache();
 assert.equal(f.calls.length,1);assert.equal(f.calls[0].kind,'dns_cache');assert.deepEqual(f.calls[0].params,{domain:'a.test',limit:20});assert.deepEqual(f.calls[0].expectedScope,scope());
 dns.cacheDomain='b.test';f.emit(job(101,'dns_cache','completed',scope(),{enabled:true,domain:'a.test',hit:false,addresses:[],ttl_seconds:null},2));await flush();assert.equal(dns.cacheResult.domain,'a.test');dns.dispose();
});
test('FakeIP query no longer triggers a DNS cache read',async()=>{
 const f=fixture(),dns=new DnsState(f.ports);await dns.jobs.init();dns.fakeQuery='a.test';await dns.runFakeIp();assert.deepEqual(f.calls.map(c=>c.kind),['fake_ip_lookup']);dns.dispose();
});
test('invalid limits, route ports or protocol do not allocate tasks',async()=>{
 const f=fixture(),dns=new DnsState(f.ports),route=new RouteTraceState(f.ports);await dns.jobs.init();await route.jobs.init();
 for(const limit of [0,501,1.5]){dns.cacheLimit=limit;await dns.runCache();assert.ok(dns.cacheError);}
 route.traceTarget='example.test';for(const port of [0,65536,1.5]){route.tracePort=port;await route.runTrace();assert.ok(route.traceError);}
 route.tracePort=443;route.traceProtocol='icmp';await route.runTrace();assert.equal(f.calls.length,0);dns.dispose();route.dispose();
});
test('cache errors remain independent from FakeIP and fresh reads can explicitly retry',async()=>{
 const f=fixture(),dns=new DnsState(f.ports);await dns.jobs.init();await dns.runCache();f.emit(job(101,'dns_cache','failed',scope(),undefined,2));await flush();assert.ok(dns.cacheError);assert.equal(dns.fakeError,null);
 await dns.runCache();assert.equal(f.calls.length,2);f.emit(job(102,'dns_cache','completed',scope(),{enabled:true,entries:[],count:0},3));await flush();assert.equal(dns.cacheError,null);assert.equal(dns.cacheResult.count,0);dns.dispose();
});
test('current route and cache results invalidate on missed ABA generation and ignore late completion',async()=>{
 for(const kind of ['route_trace','dns_cache']){const f=fixture(),state=kind==='route_trace'?new RouteTraceState(f.ports):new DnsState(f.ports);await state.jobs.init();if(kind==='route_trace'){state.traceTarget='example.test';await state.runTrace();}else await state.runCache();
 const result=kind==='route_trace'?routeResult():{enabled:true,entries:[],count:0};f.emit(job(101,kind,'completed',scope(),result,2));await flush();assert.ok(kind==='route_trace'?state.traceResult:state.cacheResult);
 f.set(snapshot(scope(),3));assert.equal(kind==='route_trace'?state.traceResult:state.cacheResult,null);f.emit(job(101,kind,'completed',scope(),result,4));await flush();assert.equal(kind==='route_trace'?state.traceResult:state.cacheResult,null);assert.equal(f.calls.length,1);state.dispose();}
});
test('incompatible result cannot become a current success or silently print arbitrary JSON',async()=>{
 for(const kind of ['route_trace','dns_cache']){const f=fixture(),state=kind==='route_trace'?new RouteTraceState(f.ports):new DnsState(f.ports);await state.jobs.init();if(kind==='route_trace'){state.traceTarget='example.test';await state.runTrace();}else await state.runCache();f.emit(job(101,kind,'completed',scope(),{secret:'secret-canary',hops:[]},2));await flush();assert.equal(kind==='route_trace'?state.traceResult:state.cacheResult,null);assert.match(kind==='route_trace'?state.traceError:state.cacheError,/格式未适配|不完整/);state.dispose();}
});
