<script lang="ts">
  import { untrack } from 'svelte';
  import WireGuardPeerFields from './WireGuardPeerFields.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import type { UserEndpoint } from '$lib/features/endpoints/library';
  import { parseAddressList, type WireGuardProtocol } from '$lib/features/endpoints/protocols/wireguard';
  let { endpoint = $bindable(), disabled = false }: { endpoint: UserEndpoint; disabled?: boolean } = $props();
  const protocol = $derived(endpoint.protocol as WireGuardProtocol);
  let addressText = $state(untrack(() => protocol.addresses.join(', ')));
</script>
<div class="grid gap-4">
  <label class="grid gap-1.5 text-xs"><span>私钥 <span class="text-destructive" aria-hidden="true">*</span></span><Input required type="password" bind:value={protocol.private_key} {disabled} autocomplete="new-password" spellcheck="false" class="font-mono" /></label>
  <label class="grid gap-1.5 text-xs"><span>本机隧道地址 <span class="text-destructive" aria-hidden="true">*</span></span><Input required bind:value={addressText} oninput={event => protocol.addresses = parseAddressList(event.currentTarget.value)} {disabled} placeholder="10.66.0.2/32, fd66::2/128" class="font-mono" /></label>
  {#each protocol.peers as peer, index (peer)}
    <WireGuardPeerFields bind:peer={protocol.peers[index]} {index} {disabled} removable={protocol.peers.length > 1} onRemove={() => protocol.peers = protocol.peers.filter((_, i) => i !== index)} />
  {/each}
  <Button type="button" variant="outline" size="sm" {disabled} onclick={() => protocol.peers.push({ public_key: '', allowed_ips: [] })}>添加 Peer</Button>
</div>
