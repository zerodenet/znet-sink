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
  assert.ok(panel.includes(`value="${protocol}"`), `DNS panel must expose ${protocol}`);
}
assert.match(panel, /First-match-wins/);
assert.match(panel, /DoH \/ DoT \/ DoQ/);
assert.match(panel, /aria-pressed=\{draft\.mode === item\[0\]\}/);
assert.match(runtime, /json!\(tun\.dns_hijack\)/);
assert.match(parsing, /"original_ip", "originalIp"/);
assert.match(parsing, /"fake_ip_reverse_status", "fakeIpReverseStatus"/);

console.log('dns-config: ok');
