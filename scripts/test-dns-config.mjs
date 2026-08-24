import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const service = readFileSync('src/lib/services/dns-config.ts', 'utf8');
const panel = readFileSync('src/lib/components/settings/DnsSettingsPanel.svelte', 'utf8');
const runtime = readFileSync('src-tauri/src/kernel/zero/runtime.rs', 'utf8');
const parsing = readFileSync('src-tauri/src/kernel/zero/parsing.rs', 'utf8');

assert.match(service, /enabled: draft\.mode !== 'disabled'/);
assert.match(service, /config: clone\(draft\.dns\)/);
assert.match(service, /export function setDnsMode\(draft: DnsSettingsDraft, mode: DnsMode\)/);
assert.match(service, /rule\.server === oldName \? name : rule\.server/);
assert.match(service, /guiValidateDnsConfig\(next\)/);
assert.match(service, /const result = await guiApplyDnsConfig\(next\)/);

for (const protocol of ['udp', 'doh', 'dot', 'doq', 'system']) {
  assert.ok(panel.includes(`value: '${protocol}'`), `DNS panel must expose ${protocol}`);
}
assert.match(panel, /First-match-wins/);
assert.match(panel, /DoH \/ DoT \/ DoQ/);
assert.match(panel, /role="radiogroup" aria-label="DNS 基础模式"/);
assert.match(panel, /aria-checked=\{draft\.mode === item\[0\]\}/);
assert.match(panel, /<Dialog\.Title>\{editingServerName \? '编辑 DNS 服务器' : '新增 DNS 服务器'\}<\/Dialog\.Title>/);
assert.match(panel, /<Dialog\.Title>编辑 Zero 原生 DNS JSON<\/Dialog\.Title>/);
assert.match(panel, /应用到表单/);
assert.doesNotMatch(panel, /structuredClone/);
assert.match(panel, /function cloneDnsValue<T>\(value: T\): T/);
assert.match(panel, /function openAddDispatch\(\)/);
assert.match(panel, /function openEditDispatch\(index: number\)/);
assert.match(panel, /class="dispatch-dialog-form"/);
assert.match(panel, /function buildDispatchConditionFromForm\(\): Record<string, unknown>/);
assert.match(panel, /role="tablist" aria-label="DNS 分流条件编辑方式"/);
assert.match(panel, /switchDispatchEditorMode\('form'\)/);
assert.match(panel, /switchDispatchEditorMode\('json'\)/);
assert.match(panel, /const condition = \{ type: 'domain', values: \['example\.com'\] \}/);
assert.doesNotMatch(panel, /updateDispatchCondition/);
assert.match(runtime, /json!\(tun\.dns_hijack\)/);
assert.match(parsing, /"original_ip", "originalIp"/);
assert.match(parsing, /"fake_ip_reverse_status", "fakeIpReverseStatus"/);

console.log('dns-config: ok');
