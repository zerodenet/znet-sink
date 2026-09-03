import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { effectiveConfigDiff } from '../src/lib/services/config-diff.ts';
import { projectClientKernelFeatures } from '../src/lib/services/kernel-capabilities.ts';
import {
  DNS_DETOUR_ROUTE_FINAL,
  createDefaultDnsConfig,
  createRecommendedDnsConfig,
  parseDnsConfig,
  projectDnsSettings,
  readDnsSettings,
  recommendedDnsAddressFamily,
  setDnsMode,
  validateDnsDraft,
} from '../src/lib/services/dns-config.ts';

const service = readFileSync('src/lib/services/dns-config.ts', 'utf8');
const panel = readFileSync('src/lib/components/settings/DnsSettingsPanel.svelte', 'utf8');
const selectContent = readFileSync('src/lib/components/ui/select/select-content.svelte', 'utf8');
const runtime = readFileSync('src-tauri/src/kernel/zero/runtime.rs', 'utf8');
const parsing = readFileSync('src-tauri/src/kernel/zero/parsing.rs', 'utf8');
const guiCore = readFileSync('src-tauri/src/commands/gui_core.rs', 'utf8');
const dnsTransaction = readFileSync('src-tauri/src/commands/gui_core/dns_transaction.rs', 'utf8');
const tunPanel = readFileSync('src/lib/components/settings/TunSettingsPanel.svelte', 'utf8');
const recoveryActions = readFileSync('src/lib/components/core/ErrorRecoveryActions.svelte', 'utf8');
const configService = readFileSync('src/lib/services/config.ts', 'utf8');
const ruleOverlay = readFileSync('src-tauri/src/services/rule_overlay.rs', 'utf8');
const proxyConfig = readFileSync('src-tauri/src/services/proxy_config.rs', 'utf8');

assert.match(service, /enabled: draft\.mode !== 'disabled'/);
assert.match(service, /config: clone\(draft\.dns\)/);
assert.match(service, /export function setDnsMode\(/);
assert.match(service, /rule\.server === oldName \? name : rule\.server/);
assert.match(service, /guiValidateDnsConfig\(next\)/);
assert.match(service, /const result = await guiApplyDnsConfig\(next\)/);
assert.match(service, /answer\.exclude_domains = Array\.isArray\(answer\.exclude_domains\)/);
assert.match(service, /policy: \{ address_family: defaults\.addressFamily \?\? 'prefer_ipv4' \}/);
assert.match(service, /servers: \{ system: createDnsServer\('system'\) \}/);
assert.match(service, /default_server: 'system'/);
assert.match(service, /ipv6_cidr: previous\?\.ipv6_cidr/);
assert.match(service, /export function getDnsAddressFamilyPolicy\(dns: DnsConfig\)/);
assert.match(service, /export function setDnsAddressFamilyPolicy\(/);
assert.match(service, /field: 'policy\.address_family'/);
assert.match(service, /routeTargetTags\?: ReadonlySet<string>/);
assert.match(service, /detour !== DNS_DETOUR_ROUTE_FINAL/);
assert.match(service, /policy\.node_server/);
assert.match(service, /policy\.direct_server/);
assert.match(service, /policy\.fallback_servers/);
assert.match(service, /policy\.server_timeout_ms/);
assert.match(service, /节点解析服务器 .* 不能再通过 detour 转发/);
assert.match(service, /DoQ 暂不支持通过出站转发/);
assert.match(service, /reverse_mapping\.max_domains_per_address/);
assert.match(service, /context\.ruleSetTags && !context\.ruleSetTags\.has\(tag\)/);
assert.match(service, /tunDnsSystemAuto\.state === 'unsupported'/);
assert.match(service, /dnsFakeIpDualStack\.state === 'unsupported'/);
assert.match(service, /dnsRealReverseMapping\.state === 'unsupported'/);
assert.match(service, /ipv6_cidr: 'fd00::\/96'/);
assert.match(service, /ipv4Availability === 'available' && ipv6Availability === 'unavailable'/);
assert.match(service, /ipv4Availability === 'unavailable' && ipv6Availability === 'available'/);

for (const protocol of ['udp', 'doh', 'dot', 'doq', 'system']) {
  assert.ok(panel.includes(`value: '${protocol}'`), `DNS panel must expose ${protocol}`);
}
assert.match(panel, /按顺序匹配域名或规则集/);
assert.match(panel, /dns_encrypted_client_queries_not_intercepted/);
assert.match(panel, /dns_ech_hostname_recovery_unavailable/);
assert.match(panel, /<SegmentedControl\.Root value=\{draft\.mode\}[\s\S]*?aria-label="DNS 基础模式"/);
assert.match(panel, /<SegmentedControl\.Item value=\{item\[0\]\}/);
assert.match(panel, /aria-label="DNS 应答地址族策略"/);
assert.match(panel, /aria-label="DNS 上游经由出站"/);
assert.match(panel, /跟随默认出站/);
assert.match(panel, /aria-label="节点解析服务器"/);
assert.match(panel, /aria-label="直连解析服务器"/);
assert.match(panel, /通用回退链/);
assert.match(panel, /单独超时/);
assert.doesNotMatch(panel, /getConfigProxyNodes/);
assert.match(panel, /getConfigPolicyGroups/);
assert.match(panel, /targets\.set\('block'/);
assert.match(panel, /策略组 ·/);
assert.match(panel, /aria-label="真实地址映射"/);
assert.match(panel, /changeReverseMapping\('max_domains_per_address'/);
assert.match(panel, /getGuiTunStatus/);
assert.match(panel, /automaticAddressFamilyPolicy/);
for (const policy of ['prefer_ipv4', 'prefer_ipv6', 'ipv4_only', 'ipv6_only']) {
  assert.ok(panel.includes(`value: '${policy}'`), `DNS panel must expose ${policy}`);
}
assert.match(panel, /<Dialog\.Title>\{editingServerName \? '编辑 DNS 服务器' : '新增 DNS 服务器'\}<\/Dialog\.Title>/);
assert.match(panel, /<Dialog\.Title>编辑内核原生 DNS JSON<\/Dialog\.Title>/);
assert.match(panel, /应用到表单/);
assert.doesNotMatch(panel, /structuredClone/);
assert.match(panel, /function cloneDnsValue<T>\(value: T\): T/);
assert.match(panel, /function openAddDispatch\(\)/);
assert.match(panel, /function openEditDispatch\(index: number\)/);
assert.match(panel, /class="dispatch-dialog-form"/);
assert.match(panel, /function buildDispatchConditionFromForm\(\): Record<string, unknown>/);
assert.match(panel, /getEffectiveRuleSetOptions/);
assert.match(panel, /aria-label="DNS 分流规则集"/);
assert.match(panel, /只显示当前可用的规则集/);
assert.match(panel, /<SegmentedControl\.Root value=\{dispatchEditorMode\}[\s\S]*?aria-label="DNS 分流条件编辑方式"/);
assert.match(panel, /switchDispatchEditorMode\(value as 'form' \| 'json'\)/);
assert.match(panel, /<SegmentedControl\.Item value="form"/);
assert.match(panel, /<SegmentedControl\.Item value="json"/);
assert.match(panel, /const condition = \{ type: 'domain', values: \['example\.com'\] \}/);
assert.doesNotMatch(panel, /updateDispatchCondition/);
assert.match(panel, /dispatchConditionSummary\(rule\.condition\)/);
assert.doesNotMatch(panel, /<code>\{JSON\.stringify\(rule\.condition\)\}<\/code>/);
assert.match(panel, /draft\.dns\.answer\.ipv6_cidr/);
assert.match(panel, /最终有效配置/);
assert.match(panel, /高级选项/);
assert.doesNotMatch(panel, /class="config-lineage"/);
assert.match(panel, /guiInspectDnsEffectiveConfig/);
assert.match(panel, /ruleSetSignal\.onChanged/);
assert.match(service, /已不存在或未进入最终有效配置/);
assert.match(panel, /ErrorRecoveryActions/);
assert.match(panel, /\(draft\.dns\.answer\.exclude_domains \?\? \[\]\)\.join\('\\n'\)/);
assert.match(tunPanel, /inspectTunDnsHijackReadiness/);
assert.match(tunPanel, /features\?\.tunDualStack\.state === 'unsupported'/);
assert.match(recoveryActions, /guiExportDiagnostics/);
assert.match(guiCore, /pub fn gui_inspect_dns_effective_config/);
assert.match(guiCore, /compose_effective_config_with_dns/);
assert.match(guiCore, /rollback_runtime_if_owned/);
assert.match(dnsTransaction, /dnsRollback/);
assert.match(dnsTransaction, /restored_last_known_good/);
assert.match(dnsTransaction, /dnsStorageRollback/);
assert.match(configService, /ruleSetSignal\.markChanged\(\)/);
assert.match(runtime, /json!\(tun\.dns_hijack\)/);
assert.match(parsing, /"original_ip", "originalIp"/);
assert.match(parsing, /"fake_ip_reverse_status", "fakeIpReverseStatus"/);
assert.match(parsing, /"connection_attempts"/);
assert.match(parsing, /"retired_addresses"/);
assert.match(ruleOverlay, /resolve_dns_detours\(config, &mut dns\)\?/);
assert.match(ruleOverlay, /route_final_dns_detour/);
assert.match(ruleOverlay, /HashSet::from\(\["direct"\.to_owned\(\), "block"\.to_owned\(\)\]\)/);
assert.ok(selectContent.includes('max-h-[min(20rem,calc(100dvh-1rem),var(--bits-floating-available-height,20rem))]'));
assert.ok(selectContent.includes('min-h-0') && selectContent.includes('max-h-72'));
assert.doesNotMatch(selectContent, /--bits-select-/);
const activation = proxyConfig.slice(proxyConfig.indexOf('pub async fn activate_runtime'));
assert.ok(
  activation.indexOf('validate_config(content.clone(), options.clone())')
    < activation.indexOf('match adapter.apply_config(content, options).await'),
  'profile activation must validate the composed target config before hot apply or restart fallback',
);

const features = projectClientKernelFeatures({
  available: true,
  features: ['tun_dns_system_auto', 'dns_split_dispatch', 'dns_real_reverse_mapping'],
  buildFeatures: [],
  contracts: { capabilities: { minimumSupported: 1, current: 1 } },
});
assert.equal(features.tunDnsSystemAuto.state, 'supported');
assert.equal(features.dnsSplitDispatch.state, 'supported');
assert.equal(features.dnsFakeIpDualStack.state, 'unsupported');
assert.equal(features.dnsRealReverseMapping.state, 'supported');

const fullFeatures = projectClientKernelFeatures({
  available: true,
  features: [
    'tun_dns_system_auto',
    'dns_split_dispatch',
    'dns_fake_ip_dual_stack',
    'dns_real_reverse_mapping',
  ],
  buildFeatures: [],
  contracts: { capabilities: { minimumSupported: 1, current: 1 } },
});

assert.equal(recommendedDnsAddressFamily('available', 'unavailable'), 'ipv4_only');
assert.equal(recommendedDnsAddressFamily('unavailable', 'available'), 'ipv6_only');
assert.equal(recommendedDnsAddressFamily('available', 'available'), 'prefer_ipv4');
assert.equal(recommendedDnsAddressFamily('unknown', 'unknown'), 'prefer_ipv4');

const capableFakeIp = createDefaultDnsConfig('fake_ip', {
  features: fullFeatures,
  addressFamily: 'ipv4_only',
});
assert.deepEqual(capableFakeIp.answer, {
  type: 'fake_ip',
  cidr: '198.18.0.0/15',
  ipv6_cidr: 'fd00::/96',
  ttl_seconds: 86_400,
  exclude_domains: [],
});
assert.deepEqual(capableFakeIp.reverse_mapping, {
  max_entries: 1024,
  max_domains_per_address: 8,
  max_ttl_seconds: 300,
});
assert.equal(capableFakeIp.policy?.address_family, 'ipv4_only');

const legacyFakeIp = createDefaultDnsConfig('fake_ip');
assert.equal(legacyFakeIp.answer.type, 'fake_ip');
assert.equal(legacyFakeIp.answer.ipv6_cidr, undefined);
assert.equal(legacyFakeIp.reverse_mapping, undefined);

const recommendedDns = createRecommendedDnsConfig('real');
assert.equal(recommendedDns.default_server, 'cloudflare');
assert.deepEqual(Object.keys(recommendedDns.servers), [
  'cloudflare',
  'google',
  'cloudflare-bootstrap',
  'google-bootstrap',
  'alidns',
  '114dns',
  'system',
]);
assert.equal(recommendedDns.servers.cloudflare.detour, DNS_DETOUR_ROUTE_FINAL);
assert.equal(recommendedDns.servers.google.detour, DNS_DETOUR_ROUTE_FINAL);
assert.deepEqual(recommendedDns.policy?.fallback_servers, ['google', 'system']);
assert.equal(recommendedDns.policy?.node_server, 'system');
assert.deepEqual(recommendedDns.policy?.node_fallback_servers, [
  'cloudflare-bootstrap',
  'google-bootstrap',
]);
assert.equal(recommendedDns.servers['cloudflare-bootstrap'].detour, undefined);
assert.equal(recommendedDns.servers['google-bootstrap'].detour, undefined);
assert.deepEqual(recommendedDns.servers.alidns, {
  type: 'doh',
  host: 'dns.alidns.com',
  port: 443,
  path: '/dns-query',
  bootstrap: ['223.5.5.5', '223.6.6.6'],
});
assert.deepEqual(recommendedDns.servers['114dns'], {
  type: 'udp',
  host: '114.114.114.114',
  port: 53,
});
assert.equal(recommendedDns.servers.alidns.detour, undefined);
assert.equal(recommendedDns.servers['114dns'].detour, undefined);

const repeatedPrimaryFallback = readDnsSettings({
  enabled: true,
  dnsHijack: false,
  config: {
    ...recommendedDns,
    policy: { ...recommendedDns.policy, fallback_servers: ['cloudflare'] },
  },
}, false);
assert.equal(
  validateDnsDraft(repeatedPrimaryFallback)
    .some((issue) => issue.field === 'policy.fallback_servers' && issue.severity === 'error'),
  true,
  'the general fallback chain must not repeat the default DNS server',
);

const sourceWithCnameTarget = {
  enabled: true,
  dnsHijack: true,
  config: {
    ...capableFakeIp,
    dispatch: [{
      condition: { type: 'domain', values: ['open.bigmodel.cn'] },
      server: 'system',
    }],
  },
};
const loaded = readDnsSettings(sourceWithCnameTarget, false);
assert.equal(loaded.mode, 'fake_ip');
assert.equal(loaded.dnsHijack, true);
assert.deepEqual(loaded.dns.dispatch, sourceWithCnameTarget.config.dispatch);
assert.deepEqual(projectDnsSettings(sourceWithCnameTarget, loaded), sourceWithCnameTarget);
assert.deepEqual(parseDnsConfig(sourceWithCnameTarget.config), sourceWithCnameTarget.config);

const followRouteFinalDraft = readDnsSettings({
  enabled: true,
  dnsHijack: false,
  config: {
    ...capableFakeIp,
    servers: {
      ...capableFakeIp.servers,
      cloudflare: {
        type: 'doh',
        host: 'cloudflare-dns.com',
        port: 443,
        path: '/dns-query',
        bootstrap: ['1.1.1.1'],
        detour: DNS_DETOUR_ROUTE_FINAL,
      },
    },
  },
}, false);
assert.equal(
  validateDnsDraft(followRouteFinalDraft, { routeTargetTags: new Set(['Proxy']) })
    .some((issue) => issue.field === 'servers.cloudflare.detour'),
  false,
  'client-only route.final detour must not be reported as a stale concrete target',
);

const realMode = setDnsMode(loaded, 'real', {
  features: fullFeatures,
  addressFamily: 'ipv4_only',
});
assert.equal(realMode.dns.answer.type, 'real');
assert.equal(realMode.dnsHijack, true);
assert.deepEqual(realMode.dns.dispatch, sourceWithCnameTarget.config.dispatch);

const disabledMode = setDnsMode(loaded, 'disabled', { features: fullFeatures });
const disabledProjection = projectDnsSettings(sourceWithCnameTarget, disabledMode);
assert.equal(disabledProjection.enabled, false);
assert.equal(disabledProjection.dnsHijack, false);
assert.deepEqual(disabledProjection.config, sourceWithCnameTarget.config);

const restoredFakeIp = setDnsMode(realMode, 'fake_ip', {
  features: fullFeatures,
  addressFamily: 'ipv6_only',
});
assert.equal(restoredFakeIp.dns.answer.type, 'fake_ip');
assert.equal(restoredFakeIp.dns.answer.ipv6_cidr, 'fd00::/96');
assert.equal(restoredFakeIp.dns.policy?.address_family, 'ipv6_only');
assert.equal(restoredFakeIp.dnsHijack, true);

const diff = effectiveConfigDiff(
  { runtime: { dns: { enabled: false } } },
  { runtime: { dns: { enabled: true } } },
  [{ id: 'dns', label: '全局 DNS 覆盖', enabled: true, paths: ['runtime.dns'] }],
);
assert.deepEqual(diff, [{
  path: 'runtime.dns.enabled',
  source: '全局 DNS 覆盖',
  before: false,
  after: true,
}]);

console.log('dns-config: ok');
