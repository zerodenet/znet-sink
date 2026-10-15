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
const cleared = { enabled: true, removedMappings: 4, removedAddresses: 4, liveMappings: 0, retiredAddresses: 0 };

test('restored completed jobs remain readable history and never repopulate current DNS or spawn work', async () => {
  const f = fixture([
    job(1, 'dns_lookup', 'completed', scope('old'), { answers: [{ value: '192.0.2.1' }] }),
    job(2, 'fake_ip_clear', 'completed', scope('old'), cleared),
    job(3, 'fake_ip_clear', 'completed', scope(), cleared),
  ]);
  const dns = new DnsState(f.ports); await dns.jobs.init(); await flush();
  assert.equal(dns.dnsResult, null); assert.equal(dns.fakeMessage, null); assert.equal(f.calls.length, 0);
  assert.equal(dns.jobs.history().length, 3); assert.equal(dns.jobs.latest('fake_ip_clear'), null); dns.dispose();
});

test('restored route result does not become a current route evaluation', async () => {
  const f = fixture([job(1, 'route_trace', 'completed', scope(), { route: 'old-direct' })]);
  const route = new RouteTraceState(f.ports); await route.jobs.init();
  assert.equal(route.traceResult, null); assert.equal(route.jobs.history().length, 1); route.dispose();
});

test('current running task can recover then deliver a live completion without replay', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running')]);
  const dns = new DnsState(f.ports); await dns.jobs.init(); assert.equal(dns.dnsLoading, true);
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: [{ value: '192.0.2.1' }] }, 2)); await flush();
  assert.equal(dns.dnsResult.answers[0].value, '192.0.2.1'); assert.equal(dns.dnsLoading, false); assert.equal(f.calls.length, 0); dns.dispose();
});

test('profile, lossless revision and instance transitions clear current facts and refuse old controls', async () => {
  for (const next of [scope('two'), scope('one', '18446744073709551615'), scope('one', '18446744073709551614', 5)]) {
    const f = fixture([job(1, 'dns_lookup', 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
    f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: [] }, 2)); await flush(); assert.ok(dns.dnsResult);
    f.set(snapshot(next, 2)); assert.equal(dns.dnsResult, null);
    f.emit(job(1, 'fake_ip_clear', 'completed', scope(), cleared, 4)); await flush();
    assert.equal(dns.fakeMessage, null); assert.equal(f.calls.length, 0);
    await assert.rejects(() => dns.jobs.cancel(1)); assert.equal(f.cancellations.length, 0); dns.dispose();
  }
});

test('one live clear refreshes cache once with its original scope; duplicate completion does not replay', async () => {
  const f = fixture(); const dns = new DnsState(f.ports); await dns.jobs.init(); dns.fakeQuery = 'same.example';
  await dns.clearFakeIp('selected'); const id = [...dns.jobs.jobs.keys()][0];
  f.emit(job(id, 'fake_ip_clear', 'completed', scope(), cleared, 3)); await flush();
  assert.match(dns.fakeMessage, /已删除 4 个映射/); assert.deepEqual(f.calls.map(call => call.kind), ['fake_ip_clear', 'dns_cache']);
  assert.deepEqual(f.calls[1].expectedScope, scope());
  f.emit(job(id, 'fake_ip_clear', 'completed', scope(), cleared, 3)); await flush(); assert.equal(f.calls.length, 2); dns.dispose();
});

test('scope changing after a live clear cannot retarget its derived cache request', async () => {
  const f = fixture(); const dns = new DnsState(f.ports); await dns.jobs.init();
  const original = scope(); f.set(snapshot(scope('replacement'), 2));
  await assert.rejects(() => dns.jobs.start({ kind: 'dns_cache', params: { limit: 50 }, expectedScope: original }));
  assert.equal(f.calls.length, 0); dns.dispose();
});

test('late start success, failure and scope reads cannot mutate a replacement page', async () => {
  for (const failure of [false, true]) {
    const f = fixture(), pending = deferred(); f.ports.start = () => pending.promise;
    const dns = new DnsState(f.ports); await dns.jobs.init(); dns.dnsHost = 'same.example';
    const started = dns.runDns(); await flush(); f.set(snapshot(scope('new'), 2));
    if (failure) pending.reject(new Error('old request error'));
    else pending.resolve(job(101, 'dns_lookup', 'completed', scope(), { answers: ['old'] }, 2));
    await started; assert.equal(dns.dnsResult, null); assert.equal(dns.dnsError, null); assert.equal(dns.dnsLoading, false);
    dns.jobs.bind(snapshot(scope(), 1)); assert.deepEqual(dns.jobs.scope, scope('new')); dns.dispose();
  }
});

test('current cancellation carries exact scope, historical task never sends cancel', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running'), job(2, 'dns_lookup', 'running', scope('old'))]);
  const dns = new DnsState(f.ports); await dns.jobs.init(); await dns.cancel(1);
  assert.deepEqual(f.cancellations, [{ id: 1, expected: scope() }]);
  await assert.rejects(() => dns.jobs.cancel(2)); assert.equal(f.cancellations.length, 1); dns.dispose();
});

test('a missing scope or offline source invalidates current facts but preserves history without issuing work', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: [] }, 2)); await flush();
  f.set(snapshot(scope(), 2, 'offline')); assert.equal(dns.dnsResult, null); assert.equal(dns.jobs.ready, false);
  f.set(null); dns.dnsHost = 'same.example'; await dns.runDns(); assert.equal(f.calls.length, 0); assert.equal(dns.jobs.history().length, 1); dns.dispose();
});

test('a completion arriving before the start receipt is projected once after its request identity is known', async () => {
  const f = fixture(), pending = deferred(); f.ports.start = () => pending.promise;
  const dns = new DnsState(f.ports); await dns.jobs.init(); dns.fakeQuery = 'same.example';
  const start = dns.clearFakeIp('selected'); await flush();
  f.emit(job(101, 'fake_ip_clear', 'completed', scope(), cleared, 2)); await flush(); assert.equal(dns.fakeMessage, null);
  // Cache calls are deliberately refused, while the original late receipt is retained.
  f.ports.start = async request => { f.calls.push(request); throw new Error('cache unavailable'); };
  pending.resolve(job(101, 'fake_ip_clear', 'queued')); await start; await flush();
  assert.match(dns.fakeMessage, /已删除 4 个映射/); assert.equal(f.calls.length, 1); assert.equal(dns.fakeError, 'cache unavailable'); dns.dispose();
});

test('disposing a page releases both late subscriptions and never cancels backend execution', async () => {
  const f = fixture(), subscription = deferred(); let stops = 0;
  f.ports.subscribeScope = () => subscription.promise;
  const owner = new ToolJobsState(['dns_lookup'], f.ports); const initializing = owner.init(); await flush();
  owner.dispose(); subscription.resolve(() => stops++); await initializing;
  assert.equal(stops, 1); assert.equal(f.cancellations.length, 0); assert.equal(owner.ready, false);
});

test('history projection is bounded and retains active current jobs over older terminal history', async () => {
  const history = Array.from({ length: 200 }, (_, index) => job(index + 1, 'dns_lookup', 'completed'));
  history.unshift(job(0, 'dns_lookup', 'running'));
  const f = fixture(history); const owner = new ToolJobsState(['dns_lookup'], f.ports); await owner.init();
  assert.equal(owner.jobs.size, 128); assert.equal(owner.active().length, 1); assert.equal(owner.active()[0].id, 0); owner.dispose();
});

test('completion racing the initial active snapshot is recovered from the newer actual record', async () => {
  const f = fixture(), pending = deferred(); f.ports.list = () => pending.promise;
  const dns = new DnsState(f.ports); await flush();
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: ['new-completion'] }, 2)); await flush();
  assert.equal(dns.dnsResult, null);
  pending.resolve([job(1, 'dns_lookup', 'running')]); await dns.jobs.init();
  assert.deepEqual(dns.dnsResult, { answers: ['new-completion'] }); assert.equal(f.calls.length, 0); dns.dispose();
});

test('temporary scope read failure re-discovers current active jobs before accepting their completion', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
  const read = f.ports.scope; f.ports.scope = async () => { throw new Error('read interrupted'); };
  await assert.rejects(() => dns.jobs.refreshScope()); assert.equal(dns.jobs.ready, false);
  f.ports.scope = read; await dns.jobs.refreshScope(); await flush(); assert.equal(dns.jobs.active().length, 1);
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: ['recovered'] }, 2)); await flush();
  assert.deepEqual(dns.dnsResult, { answers: ['recovered'] }); assert.equal(f.calls.length, 0); dns.dispose();
});

test('A to B to A cannot make a rejected late start or subsequent completion current again', async () => {
  const f = fixture(), pending = deferred(); f.ports.start = () => pending.promise;
  const dns = new DnsState(f.ports); await dns.jobs.init(); dns.dnsHost = 'same.example';
  const started = dns.runDns(); await flush();
  f.set(snapshot(scope('B'), 2)); f.set(snapshot(scope(), 3));
  pending.resolve(job(101, 'dns_lookup', 'queued')); await started;
  f.emit(job(101, 'dns_lookup', 'completed', scope(), { answers: ['old-A'] }, 2)); await flush();
  assert.equal(dns.dnsResult, null); assert.equal(dns.jobs.active().length, 0);
  assert.equal(dns.jobs.history().find(job => job.id === 101).state, 'completed'); dns.dispose();
});

test('a preflight crossing A to B to A refuses the original intent before dispatch', async () => {
  const f = fixture(), pending = deferred(); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.ports.scope = () => pending.promise; dns.dnsHost = 'same.example'; const started = dns.runDns(); await flush();
  f.set(snapshot(scope('B'), 2)); f.set(snapshot(scope(), 3)); pending.resolve(snapshot()); await started;
  assert.equal(f.calls.length, 0); assert.equal(dns.dnsResult, null); dns.dispose();
});

test('a live event awaiting scope through A to B to A cannot grant itself active recovery', async () => {
  const f = fixture(), pending = deferred(); const dns = new DnsState(f.ports); await dns.jobs.init();
  const read = f.ports.scope; f.ports.scope = () => pending.promise;
  f.emit(job(55, 'dns_lookup', 'running')); await flush(); f.set(snapshot(scope('B'), 2)); f.set(snapshot(scope(), 3));
  pending.resolve(snapshot()); await flush(); f.ports.scope = read;
  f.emit(job(55, 'dns_lookup', 'completed', scope(), { answers: ['cross-epoch'] }, 2)); await flush();
  assert.equal(dns.dnsResult, null); assert.equal(dns.jobs.active().length, 0); dns.dispose();
});

test('equal-timestamp earlier stage cannot downgrade an already completed current task', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: ['confirmed'] }, 2)); await flush();
  f.emit(job(1, 'dns_lookup', 'queued', scope(), undefined, 2)); await flush();
  assert.equal(dns.jobs.jobs.get(1).state, 'completed'); assert.deepEqual(dns.dnsResult, { answers: ['confirmed'] }); dns.dispose();
});

test('an unsafe client snapshot revision cannot authorize new work', async () => {
  const f = fixture(); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.set(snapshot(scope(), Number.MAX_SAFE_INTEGER + 1)); dns.dnsHost = 'same.example'; await dns.runDns();
  assert.equal(dns.jobs.ready, false); assert.equal(f.calls.length, 0); dns.dispose();
});

test('an unobserved A to B to A lifecycle generation revokes old completed delivery and derived clear work', async () => {
  for (const kind of ['dns_lookup', 'fake_ip_clear']) {
    const f = fixture([job(1, kind, 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
    // The UI never saw B. Scope content and instance fields equal the old A.
    f.set(snapshot(scope(), 3));
    f.emit(job(1, kind, 'completed', scope(), kind === 'dns_lookup' ? { answers: ['old-generation'] } : cleared, 4)); await flush();
    assert.equal(dns.dnsResult, null); assert.equal(dns.fakeMessage, null); assert.equal(f.calls.length, 0);
    assert.equal(dns.jobs.active().length, 0); dns.dispose();
  }
});

test('ordinary observation revision updates preserve the current lifecycle and pending delivery', async () => {
  const f = fixture([job(1, 'dns_lookup', 'running')]); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.set({ ...snapshot(scope(), 3), scopeGeneration: 1 });
  f.emit(job(1, 'dns_lookup', 'completed', scope(), { answers: ['same-generation'] }, 4)); await flush();
  assert.deepEqual(dns.dnsResult, { answers: ['same-generation'] }); dns.dispose();
});

test('a late failure from an old scope read cannot clear a newer verified generation', async () => {
  const f = fixture(), pending = deferred(); const dns = new DnsState(f.ports); await dns.jobs.init();
  f.ports.scope = () => pending.promise; const read = dns.jobs.refreshScope();
  f.set(snapshot(scope('B'), 2)); pending.reject(new Error('old read failure')); await assert.rejects(() => read);
  assert.equal(dns.jobs.ready, true); assert.deepEqual(dns.jobs.scope, scope('B')); dns.dispose();
});

test('an older host without a lifecycle generation remains readable but cannot authorize task submission', async () => {
  const f = fixture([job(1, 'dns_lookup', 'completed')]); const dns = new DnsState(f.ports); await dns.jobs.init();
  const unsupported = snapshot(); delete unsupported.scopeGeneration; f.set(unsupported);
  dns.dnsHost = 'same.example'; await dns.runDns(); assert.equal(dns.jobs.ready, false); assert.equal(f.calls.length, 0); assert.equal(dns.jobs.history().length, 1); dns.dispose();
});
