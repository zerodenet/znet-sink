import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { effectiveConfigDiff } from '../src/lib/services/config-diff.ts';
import { projectClientKernelFeatures } from '../src/lib/services/kernel-capabilities.ts';

const service = readFileSync('src/lib/services/dns-config.ts', 'utf8');
const panel = readFileSync('src/lib/components/settings/DnsSettingsPanel.svelte', 'utf8');
const runtime = readFileSync('src-tauri/src/kernel/zero/runtime.rs', 'utf8');
const parsing = readFileSync('src-tauri/src/kernel/zero/parsing.rs', 'utf8');
const guiCore = readFileSync('src-tauri/src/commands/gui_core.rs', 'utf8');
const tunPanel = readFileSync('src/lib/components/settings/TunSettingsPanel.svelte', 'utf8');
const recoveryActions = readFileSync('src/lib/components/core/ErrorRecoveryActions.svelte', 'utf8');
const configService = readFileSync('src/lib/services/config.ts', 'utf8');

assert.match(service, /enabled: draft\.mode !== 'disabled'/);
assert.match(service, /config: clone\(draft\.dns\)/);
assert.match(service, /export function setDnsMode\(draft: DnsSettingsDraft, mode: DnsMode\)/);
assert.match(service, /rule\.server === oldName \? name : rule\.server/);
assert.match(service, /guiValidateDnsConfig\(next\)/);
assert.match(service, /const result = await guiApplyDnsConfig\(next\)/);
assert.match(service, /answer\.exclude_domains = Array\.isArray\(answer\.exclude_domains\)/);
assert.match(service, /policy: \{ address_family: 'prefer_ipv4' \}/);
assert.match(service, /servers: \{ system: createDnsServer\('system'\) \}/);
assert.match(service, /default_server: 'system'/);
assert.match(service, /ipv6_cidr: previous\?\.ipv6_cidr/);
assert.match(service, /export function getDnsAddressFamilyPolicy\(dns: DnsConfig\)/);
assert.match(service, /export function setDnsAddressFamilyPolicy\(/);
assert.match(service, /field: 'policy\.address_family'/);
assert.match(service, /routeTargetTags\?: ReadonlySet<string>/);
assert.match(service, /policy\.node_server/);
assert.match(service, /policy\.direct_server/);
assert.match(service, /policy\.fallback_servers/);
assert.match(service, /policy\.server_timeout_ms/);
assert.match(service, /节点解析服务器 .* 不能再通过 detour 转发/);
assert.match(service, /DoQ 暂不支持通过出站转发/);
assert.match(service, /context\.ruleSetTags && !context\.ruleSetTags\.has\(tag\)/);
assert.match(service, /tunDnsSystemAuto\.state === 'unsupported'/);
assert.match(service, /dnsFakeIpDualStack\.state === 'unsupported'/);

for (const protocol of ['udp', 'doh', 'dot', 'doq', 'system']) {
  assert.ok(panel.includes(`value: '${protocol}'`), `DNS panel must expose ${protocol}`);
}
assert.match(panel, /按列表顺序优先匹配/);
assert.match(panel, /dns_encrypted_client_queries_not_intercepted/);
assert.match(panel, /dns_ech_hostname_recovery_unavailable/);
assert.match(panel, /role="radiogroup" aria-label="DNS 基础模式"/);
assert.match(panel, /aria-checked=\{draft\.mode === item\[0\]\}/);
assert.match(panel, /aria-label="DNS 应答地址族策略"/);
assert.match(panel, /aria-label="DNS 上游经由出站"/);
assert.match(panel, /aria-label="节点解析服务器"/);
assert.match(panel, /aria-label="直连解析服务器"/);
assert.match(panel, /通用回退链/);
assert.match(panel, /单独超时/);
assert.match(panel, /getConfigProxyNodes/);
assert.match(panel, /getConfigPolicyGroups/);
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
assert.match(panel, /无需手工推导/);
assert.match(panel, /role="tablist" aria-label="DNS 分流条件编辑方式"/);
assert.match(panel, /switchDispatchEditorMode\('form'\)/);
assert.match(panel, /switchDispatchEditorMode\('json'\)/);
assert.match(panel, /const condition = \{ type: 'domain', values: \['example\.com'\] \}/);
assert.doesNotMatch(panel, /updateDispatchCondition/);
assert.match(panel, /draft\.dns\.answer\.ipv6_cidr/);
assert.match(panel, /基础配置/);
assert.match(panel, /客户端覆盖/);
assert.match(panel, /最终有效配置/);
assert.match(panel, /查看最终配置/);
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
assert.match(configService, /ruleSetSignal\.markChanged\(\)/);
assert.match(runtime, /json!\(tun\.dns_hijack\)/);
assert.match(parsing, /"original_ip", "originalIp"/);
assert.match(parsing, /"fake_ip_reverse_status", "fakeIpReverseStatus"/);
assert.match(parsing, /"connection_attempts"/);
assert.match(parsing, /"retired_addresses"/);

const features = projectClientKernelFeatures({
  available: true,
  features: ['tun_dns_system_auto', 'dns_split_dispatch'],
  buildFeatures: [],
  contracts: { capabilities: { minimumSupported: 1, current: 1 } },
});
assert.equal(features.tunDnsSystemAuto.state, 'supported');
assert.equal(features.dnsSplitDispatch.state, 'supported');
assert.equal(features.dnsFakeIpDualStack.state, 'unsupported');

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
