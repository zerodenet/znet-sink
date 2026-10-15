import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  locationDecimal,
  validLocationSnapshot,
  sameLocationBinding,
  sameLocationRead,
  locationScopeMatches,
  locationSnapshotMatches,
  locateNodeChain,
} from '../src/lib/features/mobile/location.ts';

const max = '18446744073709551615';
const binding = (changes = {}) => ({
  binding: 'host-session-a:scope-generation-2:directory-a',
  profileId: 'profile-a', configRevision: max, coreInstanceId: '9007199254740993',
  scopeGeneration: '2', ...changes,
});
const location = (b = binding()) => ({ binding: b, profile: { id: b.profileId, name: 'same visible name' }, reason: null });
const scope = (changes = {}) => ({ profileId: 'profile-a', configRevision: max, coreInstanceId: '9007199254740993', ...changes });
const id = (tag, changes = {}) => ({ profileId: 'profile-a', configRevision: max, tag, ...changes });
const node = (tag, changes = {}) => ({
  id: id(tag), tag, protocol: 'shadowsocks', groupTags: [], selectedIn: [],
  runtimeAvailable: true, activeProbeJobIds: [], history: [], actionValid: true, ...changes,
});
const group = (tag, kind, memberTags, selected, changes = {}) => ({
  id: id(tag), tag, kind, selected, memberTags, runtimeAvailable: true, available: true, ...changes,
});
const screen = (groups = [], nodes = [], changes = {}) => ({
  revision: 7, scope: scope(), sourceStatus: 'ready', groups, nodes, activeProbeJobs: [], ...changes,
});
function unknown(snapshot, root, expression) {
  const result = locateNodeChain(snapshot, binding(), root);
  assert.equal(result.leafTag, null);
  assert.match(result.reason, expression);
  return result;
}

test('decimal identities preserve every u64 string and reject malformed or unbounded input', () => {
  for (const value of ['0', '1', '9007199254740993', max]) assert.equal(locationDecimal(value), value);
  for (const value of ['', '00', '01', '-1', '+1', '1.0', '1e3', ' 1', '1 ', `${max}0`, '18446744073709551616', '9'.repeat(100_000), null, undefined, true, {}, 1n]) {
    assert.equal(locationDecimal(value), null);
  }
});
test('legacy numeric identities are accepted only when their original value is exactly representable', () => {
  for (const value of [0, 1, Number.MAX_SAFE_INTEGER]) assert.equal(locationDecimal(value), String(value));
  for (const value of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1, Number(max), NaN, Infinity, -Infinity]) assert.equal(locationDecimal(value), null);
});
test('host snapshot requires string u64 fields and a positive generation without losing max values', () => {
  assert.equal(validLocationSnapshot(location(binding({ configRevision: max, coreInstanceId: max, scopeGeneration: max }))), true);
  for (const field of ['configRevision', 'coreInstanceId', 'scopeGeneration']) {
    for (const value of [0, 1, Number.MAX_SAFE_INTEGER + 1, '01', '18446744073709551616', '9'.repeat(100_000)]) {
      assert.equal(validLocationSnapshot(location(binding({ [field]: value }))), false);
    }
  }
  assert.equal(validLocationSnapshot(location(binding({ scopeGeneration: '0' }))), false);
  assert.equal(validLocationSnapshot(location(binding({ binding: '' }))), false);
  assert.equal(validLocationSnapshot({ ...location(), profile: { id: 'other-id', name: 'same visible name' } }), false);
});
test('opaque host, generation and metadata changes invalidate a visit even when visible scope matches', () => {
  const original = binding();
  assert.equal(sameLocationBinding(original, { ...original }), true);
  for (const changes of [
    { binding: 'host-session-b:scope-generation-2:directory-a' },
    { binding: 'host-session-a:scope-generation-4:directory-a', scopeGeneration: '4' },
    { binding: 'host-session-a:scope-generation-2:renamed-or-relinked' },
    { profileId: 'same-name-different-id' }, { configRevision: '1' }, { coreInstanceId: '1' },
  ]) {
    assert.equal(sameLocationBinding(original, binding(changes)), false);
    assert.equal(sameLocationRead(location(original), location(binding(changes))), false);
  }
  assert.equal(sameLocationBinding(null, original), false);
  assert.equal(sameLocationBinding(null, null), false);
});
test('no current profile permits only the latest list, never a named or executable target', () => {
  const latest = { binding: null, profile: null, reason: 'no current profile' };
  assert.equal(validLocationSnapshot(latest), true);
  assert.equal(locationSnapshotMatches({ kind: 'profile', profileId: null, binding: null }, latest), true);
  assert.equal(locationSnapshotMatches({ kind: 'profile', profileId: 'profile-a', binding: binding() }, latest), false);
  assert.equal(locationSnapshotMatches({ kind: 'nodes', binding: binding(), selection: { kind: 'direct' } }, latest), false);
  assert.equal(locationSnapshotMatches({ kind: 'profile', profileId: null, binding: null }, location()), false);
  assert.equal(validLocationSnapshot({ ...latest, profile: { id: 'profile-a', name: 'stale' } }), false);
  assert.equal(sameLocationRead(latest, { ...latest, reason: 'another unavailable reason' }), true);
});
test('scope matching is exact for profile, revision and instance, allowing only safe legacy numbers', () => {
  assert.equal(locationScopeMatches(binding(), scope()), true);
  for (const changed of [{ profileId: 'other' }, { configRevision: '1' }, { coreInstanceId: '1' }, { configRevision: Number(max) }, { coreInstanceId: Number('9007199254740993') }]) {
    assert.equal(locationScopeMatches(binding(), scope(changed)), false);
  }
  assert.equal(locationScopeMatches(binding({ configRevision: '7', coreInstanceId: '8' }), scope({ configRevision: 7, coreInstanceId: 8 })), true);
});
test('direct leaf uses the exact case-sensitive stable NodeScreen identity', () => {
  const snapshot = screen([], [node('direct'), node('DIRECT'), node('endpoint:wireguard')]);
  for (const tag of ['direct', 'DIRECT', 'endpoint:wireguard']) {
    assert.deepEqual(locateNodeChain(snapshot, binding(), tag), { tags: [tag], groups: [], leafTag: tag, reason: null });
  }
  unknown(snapshot, 'Direct', /移除|未提供/);
});
test('selector, urltest and fallback resolve only their declared actual selected chain', () => {
  const snapshot = screen([
    group('selector', 'selector', ['urltest', 'unused'], 'urltest'),
    group('urltest', 'url_test', ['fallback', 'unused'], 'fallback'),
    group('fallback', 'fallback', ['leaf-a', 'leaf-b'], 'leaf-b'),
  ], [node('urltest', { protocol: 'url_test' }), node('fallback', { protocol: 'fallback' }), node('leaf-a'), node('leaf-b'), node('unused')]);
  assert.deepEqual(locateNodeChain(snapshot, binding(), 'selector'), {
    tags: ['selector', 'urltest', 'fallback', 'leaf-b'], groups: ['selector', 'urltest', 'fallback'], leafTag: 'leaf-b', reason: null,
  });
  assert.equal(locateNodeChain(snapshot, binding(), 'urltest').leafTag, 'leaf-b');
});
test('selected member, membership and runtime availability are required instead of a first-node fallback', () => {
  for (const changes of [
    { selected: undefined }, { selected: '' }, { selected: 'not-a-member' },
    { runtimeAvailable: false }, { memberTags: undefined }, { memberTags: [] },
  ]) {
    unknown(screen([group('selector', 'selector', ['leaf'], 'leaf', changes)], [node('leaf')]), 'selector', /未确认/);
  }
});
test('cycle and missing selected resource keep the partial chain without inventing a leaf', () => {
  const cyclic = screen([group('a', 'selector', ['b'], 'b'), group('b', 'urltest', ['a'], 'a')], [node('b', { protocol: 'urltest' })]);
  assert.deepEqual(unknown(cyclic, 'a', /循环/).tags, ['a', 'b']);
  const missing = screen([group('a', 'fallback', ['removed'], 'removed')], [node('other')]);
  assert.deepEqual(unknown(missing, 'a', /移除|未提供/).groups, ['a']);
});
test('relay and unknown group semantics never become a unique final leaf even with a selected member', () => {
  for (const kind of ['relay', 'loadbalance', 'vendor-future-kind', '', undefined]) {
    const snapshot = screen([group('group', kind, ['leaf'], 'leaf')], [node('group', { protocol: kind }), node('leaf')]);
    unknown(snapshot, 'group', /专有语义|唯一最终节点/);
  }
});
test('same-tag group cards are legitimate NodeScreen members and do not hide their group semantics', () => {
  const snapshot = screen([group('nested', 'url-test', ['leaf'], 'leaf')], [node('nested', { protocol: 'url_test' }), node('leaf')]);
  assert.deepEqual(locateNodeChain(snapshot, binding(), 'nested'), { tags: ['nested', 'leaf'], groups: ['nested'], leafTag: 'leaf', reason: null });
});
test('a group card without its group declaration cannot masquerade as a final leaf', () => {
  for (const protocol of ['selector', 'select', 'urltest', 'url_test', 'URL-TEST', 'fallback', 'relay']) {
    const snapshot = screen([group('root', 'selector', ['group-card'], 'group-card')], [node('group-card', { protocol })]);
    const result = locateNodeChain(snapshot, binding(), 'root');
    assert.equal(result.leafTag, null, `${protocol} card must retain unknown final outlet`);
    assert.ok(result.reason);
    assert.equal(locateNodeChain(snapshot, binding(), 'group-card').leafTag, null);
  }
});
test('offline or unprovided leaf runtime preserves navigation tags without claiming an actual outlet', () => {
  for (const runtimeAvailable of [false, undefined]) {
    const snapshot = screen([group('root', 'selector', ['leaf'], 'leaf')], [node('leaf', { runtimeAvailable })]);
    const nested = locateNodeChain(snapshot, binding(), 'root');
    assert.equal(nested.leafTag, null);
    assert.deepEqual(nested.tags, ['root', 'leaf']);
    assert.deepEqual(nested.groups, ['root']);
    assert.ok(nested.reason);
    const direct = locateNodeChain(snapshot, binding(), 'leaf');
    assert.equal(direct.leafTag, null);
    assert.deepEqual(direct.tags, ['leaf']);
    assert.ok(direct.reason);
  }
});
test('available direct and future protocol leaves are not misclassified as groups', () => {
  for (const protocol of ['direct', 'wireguard', 'vendor-future-protocol', 'unknown']) {
    const snapshot = screen([], [node('leaf', { protocol })]);
    assert.deepEqual(locateNodeChain(snapshot, binding(), 'leaf'), { tags: ['leaf'], groups: [], leafTag: 'leaf', reason: null });
  }
});
test('duplicate groups or duplicate nodes are ambiguous and cannot resolve by Map overwrite', () => {
  unknown(screen([group('g', 'selector', ['a'], 'a'), group('g', 'fallback', ['b'], 'b')], [node('a'), node('b')]), 'g', /重复/);
  unknown(screen([], [node('leaf'), node('leaf')]), 'leaf', /重复/);
});
test('every resource ID must match the original profile, revision and exact tag', () => {
  for (const changes of [{ profileId: 'other' }, { configRevision: '1' }, { configRevision: Number(max) }, { tag: 'other' }]) {
    unknown(screen([], [node('leaf', { id: id('leaf', changes) })]), 'leaf', /身份/);
    unknown(screen([group('g', 'selector', ['leaf'], 'leaf', { id: id('g', changes) })], [node('leaf')]), 'g', /身份/);
  }
  unknown(screen([], [node('leaf', { id: undefined })]), 'leaf', /身份/);
});
test('an unrelated stale resource and a changed scope invalidate the directory, not just the chosen row', () => {
  unknown(screen([], [node('leaf'), node('unrelated', { id: id('unrelated', { profileId: 'old' }) })]), 'leaf', /身份/);
  for (const changes of [{ profileId: 'other' }, { configRevision: '1' }, { coreInstanceId: '1' }]) {
    unknown(screen([], [node('leaf')], { scope: scope(changes) }), 'leaf', /原定位配置或实例/);
  }
  unknown(screen([], [node('leaf')], { nodes: undefined }), 'leaf', /结构/);
});
test('ordinary NodeScreen observation revision changes preserve a stable valid chain', () => {
  const snapshot = screen([group('g', 'selector', ['leaf'], 'leaf')], [node('leaf')]);
  assert.deepEqual(locateNodeChain(snapshot, binding(), 'g'), locateNodeChain({ ...snapshot, revision: snapshot.revision + 1 }, binding(), 'g'));
});
