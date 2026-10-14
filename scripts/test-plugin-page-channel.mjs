import { readFileSync } from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  PageChannel,
  checkedDocument,
  componentFingerprint,
  publicFailure,
  boundedJson,
  PAGE_LIMITS,
} from '../src/lib/features/plugin-pages/channel.ts';
const component = (id = 'one') => ({
  plugin_id: 'test.plugin',
  component_id: id,
  name: 'Test',
  version: '1.0.0',
  publisher: 'Test',
  review: {
    key: `test.plugin/${id}`,
    identity: 'signed-hash',
    registration: 1,
    revision: 0,
  },
  enabled: true,
  running: false,
  blocked: null,
  permissions: [],
  configuration: null,
});
const doc = (components = [component()]) => ({
  plugin_id: 'test.plugin',
  page_id: 'manage',
  html: '<p>signed test</p>',
  document_review: 'host-doc-ticket',
  components,
});
const windowTarget = () => ({
  messages: [],
  postMessage(message) {
    this.messages.push(message);
  },
});
const frame = (
  id,
  method = 'component.invoke',
  args = { componentId: 'one' },
) => ({ source: 'znet-plugin-page-v1', channel: 'channel', id, method, args });
test('old generation completions cannot target a new iframe or decrement its in-flight requests', () => {
  const guard = new PageChannel(),
    old = windowTarget(),
    current = windowTarget();
  guard.open(doc(), 'channel', old);
  const first = guard.begin(old, frame(1));
  guard.close();
  guard.open(doc(), 'channel', current);
  const next = guard.begin(current, frame(2));
  assert.equal(guard.current.inFlight, 1);
  assert(!guard.result(first, { secret: 'old' }));
  guard.finish(first, false);
  guard.finish(first, false);
  assert.equal(guard.current.inFlight, 1);
  assert.deepEqual(current.messages, []);
  assert(guard.result(next, 'actual'));
  guard.finish(next, true);
  assert.equal(guard.current.inFlight, 0);
  assert.equal(current.messages.length, 1);
  assert.equal(old.messages.length, 0);
  assert.deepEqual(
    guard.history.map((h) => h.delivered),
    [false, true],
  );
});
test('explicit missing or malformed component ID never falls back; omitted ID only defaults a singleton', () => {
  const guard = new PageChannel(),
    w = windowTarget();
  guard.open(doc(), 'channel', w);
  let requestId = 0;
  for (const id of ['missing', '', null, 123]) {
    const ticket = guard.begin(
      w,
      frame(++requestId, 'configuration.get', { componentId: id }),
    );
    assert.throws(() => guard.component(ticket));
    guard.finish(ticket, false);
  }
  const good = guard.begin(w, frame(999, 'configuration.get', {}));
  assert.equal(guard.component(good).component_id, 'one');
  guard.finish(good, false);
  guard.open(doc([component(), component('two')]), 'channel', w);
  assert.throws(() =>
    guard.component(guard.begin(w, frame(1, 'configuration.get', {}))),
  );
});
test('window, channel and request IDs are fixed, seen IDs cannot replay and budgets limit real admission', () => {
  const guard = new PageChannel(),
    w = windowTarget();
  guard.open(doc(), 'channel', w);
  assert.equal(guard.begin(windowTarget(), frame(1)), null);
  assert.equal(guard.begin(w, { ...frame(1), channel: 'other' }), null);
  for (let id = 1; id <= 8; id++) assert(guard.begin(w, frame(id)));
  assert.throws(() => guard.begin(w, frame(9)), /预算/);
  guard.open(doc(), 'channel', w);
  const first = guard.begin(w, frame(1));
  guard.finish(first, true);
  assert.throws(() => guard.begin(w, frame(1)));
  for (let id = 2; id < 512; id++) {
    const request = guard.begin(w, frame(id));
    guard.finish(request, false);
  }
  assert.throws(() => guard.begin(w, frame(10000)), /预算/);
});
test('structured arguments and results enforce UTF-8 bytes, cycles, depth and total result budgets', () => {
  assert.deepEqual(
    JSON.parse(JSON.stringify(boundedJson({ text: '🙂' }, 15))),
    { text: '🙂' },
  );
  assert.throws(() => boundedJson({ text: '🙂' }, 14));
  const cycle = {};
  cycle.self = cycle;
  assert.throws(() => boundedJson(cycle, 1000));
  let deep = {};
  for (let i = 0; i < 40; i++) deep = { child: deep };
  assert.throws(() => boundedJson(deep, 1000));
  const guard = new PageChannel(),
    w = windowTarget();
  guard.open(doc(), 'channel', w);
  assert.throws(() =>
    guard.begin(
      w,
      frame(1, 'component.invoke', {
        payload: 'a'.repeat(PAGE_LIMITS.requestBytes),
      }),
    ),
  );
  const ticket = guard.begin(w, frame(2));
  assert(guard.result(ticket, 'a'.repeat(PAGE_LIMITS.resultBytes)));
  assert.equal(w.messages[0].error.code, 'budget_exceeded');
  guard.finish(ticket, true);
  for (let i = 3; i < 10; i++) {
    const t = guard.begin(w, frame(i));
    guard.result(t, 'a'.repeat(1000000));
    guard.finish(t, true);
  }
  assert(guard.current.resultBytes <= PAGE_LIMITS.totalResultBytes);
});
test('reviews remain exact safe integers and errors never echo URLs, secrets or arbitrary diagnostics', () => {
  const d = doc();
  assert(checkedDocument(d, 'test.plugin', 'manage'));
  for (const field of ['registration', 'revision']) {
    const wrong = structuredClone(d);
    wrong.components[0].review[field] = Number.MAX_SAFE_INTEGER + 1;
    assert.throws(() => checkedDocument(wrong, 'test.plugin', 'manage'));
  }
  const wrong = doc();
  wrong.components[0].plugin_id = 'other';
  assert.throws(() => checkedDocument(wrong, 'test.plugin', 'manage'));
  const failure = publicFailure({
    code: 'invalid_request',
    message: 'https://user:password@private/?token=SECRET',
    diagnostics: ['SECRET'],
    field_path: 'SECRET',
    retry_after_ms: 9999999,
  });
  assert.equal(failure.message, '参数无效，请核对输入与当前组件状态');
  assert(!JSON.stringify(failure).includes('SECRET'));
  assert.equal(failure.field_path, undefined);
  assert.equal(failure.retry_after_ms, undefined);
  const changed = doc();
  changed.components[0].enabled = false;
  assert.notEqual(
    componentFingerprint(d.components),
    componentFingerprint(changed.components),
  );
});

test('browser harness and native application share the frame navigation restriction', () => {
  const html = readFileSync(
    new URL('../src/app.html', import.meta.url),
    'utf8',
  );
  const harness = readFileSync(
    new URL('../tests/ui-controls/index.html', import.meta.url),
    'utf8',
  );
  const native = JSON.parse(
    readFileSync(
      new URL('../src-tauri/tauri.conf.json', import.meta.url),
      'utf8',
    ),
  );
  // Keep the native policy limited to frame navigation. Otherwise Tauri
  // adds asset hashes which srcdoc inherits and rejects signed guest scripts.
  assert.deepEqual(native.app.security.dangerousDisableAssetCspModification, [
    'script-src',
    'style-src',
  ]);
  assert.equal(native.app.security.csp, "frame-src 'none'");
  for (const csp of [html, harness, native.app.security.csp])
    assert.match(csp, /frame-src 'none'/);
});

test('malformed recognized frames consume admission budget before validation and cannot cause unbounded error replies', () => {
  const guard = new PageChannel(),
    w = windowTarget();
  guard.open(doc(), 'channel', w);
  for (let id = 1; id <= PAGE_LIMITS.requests; id++) {
    const malformed = { ...frame(id), method: null };
    assert.throws(() => guard.begin(w, malformed));
    const replied = guard.reject(w, malformed, { code: 'invalid_request' });
    assert.equal(replied, id < PAGE_LIMITS.requests);
  }
  assert.equal(guard.current.requests, PAGE_LIMITS.requests);
  assert.equal(guard.current.inFlight, 0);
  assert.equal(w.messages.length, PAGE_LIMITS.requests - 1);
  assert.throws(() => guard.begin(w, frame(1000)), /预算/);
  assert.equal(
    guard.reject(w, frame(1000), { code: 'invalid_request' }),
    false,
  );
  assert(guard.current.resultBytes <= PAGE_LIMITS.totalResultBytes);
});

test('typed invocation failures preserve lifecycle, budget and uncertainty codes without copying untrusted error text', () => {
  for (const code of ['disabled', 'revoked', 'expired', 'busy', 'not_found', 'transport', 'uncertain', 'deadline', 'budget_exceeded', 'cancelled', 'permission_denied']) {
    const failure = publicFailure({code, message:'https://private.invalid/?token=SECRET', diagnostics:['SECRET'],field_path:'SECRET'});
    assert.equal(failure.code, code);
    assert(!JSON.stringify(failure).includes('SECRET'));
    assert(!failure.message.includes('网络请求失败或元数据格式无效'));
    assert.equal(failure.field_path, undefined);
  }
});
