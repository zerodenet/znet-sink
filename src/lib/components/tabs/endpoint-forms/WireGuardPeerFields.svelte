<script lang="ts">
  import { untrack } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import WireGuardField from './WireGuardField.svelte';
  import { parseAddressList, type WireGuardPeer, type EndpointFieldIssue } from '$lib/features/endpoints/protocols/wireguard';
  let { peer = $bindable(), index, disabled, removable, onRemove, issues, attempted }: { peer: WireGuardPeer; index: number; disabled: boolean; removable: boolean; onRemove: () => void; issues: EndpointFieldIssue[]; attempted: boolean } = $props();
  let allowedText = $state(untrack(() => peer.allowed_ips.join(', ')));
  const field = (name: string) => `peers.${index}.${name}`;
  const issue = (name: string) => issues.find(row => row.field === field(name))?.message ?? '';
</script>
<fieldset class="grid min-w-0 gap-3 rounded-lg border border-border p-4" {disabled}>
  <legend class="px-1 text-xs font-medium">Peer {index + 1}</legend>
  <WireGuardField label="公钥" name={field('public_key')} value={peer.public_key} onValueChange={value => peer.public_key = value.trim()} required {disabled} {attempted} error={issue('public_key')} hint="对应此 Peer 的 PublicKey，填写对端的公钥。" />
  <WireGuardField label="远端地址" name={field('endpoint')} value={peer.endpoint ?? ''} onValueChange={value => { if (value.trim()) peer.endpoint = value.trim(); else delete peer.endpoint; }} {disabled} {attempted} error={issue('endpoint')} placeholder="vpn.example.com:51820" hint="对应 Endpoint；仅出站连接时必填。监听型端点的 Peer 可留空，由握手学习对端地址。" />
  <WireGuardField label="允许的 IP" name={field('allowed_ips')} value={allowedText} onValueChange={value => { allowedText = value; peer.allowed_ips = parseAddressList(value); }} required {disabled} {attempted} error={issue('allowed_ips')} placeholder="0.0.0.0/0, ::/0" hint="对应 AllowedIPs；多个 IP/前缀用逗号分隔。" />
  <WireGuardField label="预共享密钥" name={field('pre_shared_key')} value={peer.pre_shared_key ?? ''} onValueChange={value => { if (value.trim()) peer.pre_shared_key = value.trim(); else delete peer.pre_shared_key; }} secret {disabled} {attempted} error={issue('pre_shared_key')} hint="对应 PresharedKey；可选，仅对端配置了此密钥时填写。" />
  <WireGuardField label="保活间隔（秒）" name={field('keepalive_secs')} value={String(peer.keepalive_secs ?? '')} onValueChange={value => { if (value.trim()) peer.keepalive_secs = Number(value); else delete peer.keepalive_secs; }} {disabled} {attempted} error={issue('keepalive_secs')} placeholder="25" hint="对应 PersistentKeepalive；可选，0 或留空表示关闭。" />
  <Button type="button" variant="ghost" size="sm" disabled={disabled || !removable} onclick={onRemove}>移除此 Peer</Button>
</fieldset>
