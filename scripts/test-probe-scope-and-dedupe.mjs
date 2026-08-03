import assert from 'node:assert/strict';
import { planProbeTargets } from '../src/lib/components/tabs/nodes-view-model.ts';
import {
  buildDelayHistoryScope,
  planDelayHistoryScopeTransition,
  splitDelayHistoryScope,
} from '../src/lib/services/delay-history-scope.ts';

const node = (tag, protocol = 'proxy') => ({ id: tag, tag, name: tag, protocol, delay: 0, domain: 'policy' });
const group = (name, members, kind = 'selector') => ({
  name,
  kind,
  outbounds: members.map((tag) => ({ tag, type: 'proxy' })),
});

const groups = [
  group('Proxy', ['HK', 'Auto', 'SG']),
  group('Auto', ['HK', 'JP'], 'url_test'),
];
const sg = node('SG');
assert.deepEqual(planProbeTargets({
  groups,
  selectedGroup: 'Proxy',
  visibleNodes: [node('HK'), node('Auto', 'url_test'), sg],
}), { nodes: [sg], policyTags: ['Auto'] });

const nestedGroups = [
  group('Proxy', ['Auto', 'Fallback', 'US']),
  group('Auto', ['Fallback', 'JP'], 'urltest'),
  group('Fallback', ['HK']),
];
const us = node('US');
assert.deepEqual(planProbeTargets({
  groups: nestedGroups,
  selectedGroup: 'Proxy',
  visibleNodes: [node('Auto', 'urltest'), node('Fallback', 'selector'), us, { ...us, id: 'US-copy' }],
}), { nodes: [us], policyTags: ['Auto'] });

const nodes = [{ tag: 'HK', protocol: 'shadowsocks', isSelector: false }];
const profileA = buildDelayHistoryScope('profile-a', nodes, [group('Auto', ['HK'], 'url_test')]);
const profileB = buildDelayHistoryScope('profile-b', nodes, [group('Auto', ['HK'], 'url_test')]);
const changed = buildDelayHistoryScope('profile-a', nodes, [group('Auto', ['JP'], 'url_test')]);
assert.notEqual(profileA, profileB);
assert.notEqual(profileA, changed);

const emptyFingerprint = splitDelayHistoryScope(
  buildDelayHistoryScope(undefined, [], []),
).fingerprint;
const unscopedA = buildDelayHistoryScope(undefined, nodes, [group('Auto', ['HK'], 'url_test')]);
assert.deepEqual(
  planDelayHistoryScopeTransition(unscopedA, profileA, null, emptyFingerprint),
  { migrateFrom: unscopedA, provisionalScope: null },
);

const emptyA = buildDelayHistoryScope('profile-a', [], []);
assert.deepEqual(
  planDelayHistoryScopeTransition(emptyA, profileA, emptyA, emptyFingerprint),
  { migrateFrom: emptyA, provisionalScope: profileA },
);

const staleProfile = buildDelayHistoryScope('profile-a', nodes, [group('Auto', ['JP'], 'url_test')]);
const confirmedProfile = buildDelayHistoryScope('profile-b', nodes, [group('Auto', ['JP'], 'url_test')]);
assert.deepEqual(
  planDelayHistoryScopeTransition(staleProfile, confirmedProfile, staleProfile, emptyFingerprint),
  { migrateFrom: staleProfile, provisionalScope: null },
);

// Two genuinely distinct profiles with identical structure must not migrate.
assert.deepEqual(
  planDelayHistoryScopeTransition(profileA, profileB, null, emptyFingerprint),
  { provisionalScope: null },
);

console.log('probe scope and dedupe tests passed');
