<script lang="ts">
  import { untrack } from 'svelte';
  import WireGuardPeerFields from './WireGuardPeerFields.svelte';
  import WireGuardField from './WireGuardField.svelte';
  import EndpointDnsFields from './EndpointDnsFields.svelte';
  import { Button } from '$lib/components/ui/button';
  import type { UserEndpoint } from '$lib/features/endpoints/library';
  import { parseAddressList, validateWireGuardEndpoint, type WireGuardProtocol } from '$lib/features/endpoints/protocols/wireguard';
  let { endpoint = $bindable(), disabled = false, attempted = false, dnsDefaultChoice = true }: { endpoint: UserEndpoint; disabled?: boolean; attempted?: boolean; dnsDefaultChoice?: boolean } = $props();
  const protocol = $derived(endpoint.protocol as WireGuardProtocol);
  const issues = $derived(validateWireGuardEndpoint(endpoint));
  const issue = (field: string) => issues.find(row => row.field === field)?.message ?? '';
  let addressText = $state(untrack(() => protocol.addresses.join(', ')));
  const listen = $derived(endpoint.listen as { address: string; port: number } | undefined);
  $effect(() => { if (endpoint.directions?.inbound && !endpoint.listen) endpoint.listen = { address: '0.0.0.0', port: 0 }; });
</script>
<div class="grid min-w-0 gap-4">
  <WireGuardField label="私钥" name="private_key" value={protocol.private_key} onValueChange={value => protocol.private_key = value.trim()} required secret {disabled} {attempted} error={issue('private_key')} hint="对应配置的 PrivateKey，是本机身份密钥。" />
  <WireGuardField label="本机隧道地址" name="addresses" value={addressText} onValueChange={value => { addressText = value; protocol.addresses = parseAddressList(value); }} required {disabled} {attempted} error={issue('addresses')} placeholder="10.66.0.2/32, fd66::2/128" hint="对应配置的 Address，填写分配给本机的 IP/前缀；不是私钥或公钥。多个地址用逗号分隔。" />
  {#if endpoint.directions?.inbound || listen}
    <fieldset class="grid min-w-0 gap-3 rounded-lg border border-border p-4" {disabled}>
      <legend class="px-1 text-xs font-medium">本机 UDP 监听</legend>
      <WireGuardField label="监听地址" name="listen_address" value={listen?.address ?? ''} onValueChange={value => endpoint.listen = { ...listen, address: value.trim() }} required {disabled} {attempted} error={issue('listen_address')} hint="0.0.0.0 监听所有 IPv4 接口；IPv6 可填写 ::。" />
      <WireGuardField label="监听端口" name="listen_port" value={String(listen?.port ?? '')} onValueChange={value => endpoint.listen = { ...listen, port: value.trim() ? Number(value) : NaN }} required {disabled} {attempted} error={issue('listen_port')} placeholder="51820" hint="对应 ListenPort；启用入站时需要监听。0 表示由系统分配空闲端口。" />
    </fieldset>
  {/if}
  <WireGuardField label="MTU" name="mtu" value={String(protocol.mtu ?? '')} onValueChange={value => { if (value.trim()) protocol.mtu = Number(value); else delete protocol.mtu; }} {disabled} {attempted} error={issue('mtu')} placeholder="1420" hint="可选，留空使用内核默认值；优先沿用配置文件中的 MTU。" />
  {#each protocol.peers as peer, index (peer)}
    <WireGuardPeerFields bind:peer={protocol.peers[index]} {index} {disabled} {attempted} {issues} removable={protocol.peers.length > 1} onRemove={() => protocol.peers = protocol.peers.filter((_, i) => i !== index)} />
  {/each}
  <Button type="button" variant="outline" size="sm" {disabled} onclick={() => protocol.peers.push({ public_key: '', allowed_ips: [] })}>添加 Peer</Button>
  <EndpointDnsFields bind:endpoint {disabled} {attempted} error={issue} allowDefaultChoice={dnsDefaultChoice}/>
</div>
