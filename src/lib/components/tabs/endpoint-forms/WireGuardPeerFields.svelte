<script lang="ts">
  import { untrack } from 'svelte';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { parseAddressList, type WireGuardPeer } from '$lib/features/endpoints/protocols/wireguard';
  let { peer = $bindable(), index, disabled, removable, onRemove }: { peer: WireGuardPeer; index: number; disabled: boolean; removable: boolean; onRemove: () => void } = $props();
  const hintId = $props.id();
  // Preserve separators while typing; serialization uses the normalized arrays.
  let allowedText = $state(untrack(() => peer.allowed_ips.join(', ')));
</script>
<fieldset class="grid min-w-0 gap-3 rounded-lg border border-border p-4" {disabled}>
  <legend class="px-1 text-xs font-medium">Peer {index + 1}</legend>
  <label class="grid gap-1.5 text-xs"><span>公钥 <span class="text-destructive" aria-hidden="true">*</span></span><Input required bind:value={peer.public_key} {disabled} spellcheck="false" class="font-mono" /></label>
  <label class="grid gap-1.5 text-xs"><span>远端地址</span><Input aria-describedby={hintId} value={peer.endpoint ?? ''} oninput={event => { const value = event.currentTarget.value.trim(); if (value) peer.endpoint = value; else delete peer.endpoint; }} {disabled} placeholder="vpn.example.com:51820" class="font-mono" /></label><p id={hintId} class="text-xs text-muted-foreground">入站 Peer 可留空。</p>
  <label class="grid gap-1.5 text-xs"><span>允许的 IP <span class="text-destructive" aria-hidden="true">*</span></span><Input required bind:value={allowedText} oninput={event => peer.allowed_ips = parseAddressList(event.currentTarget.value)} {disabled} placeholder="0.0.0.0/0, ::/0" class="font-mono" /></label>
  <Button type="button" variant="ghost" size="sm" disabled={disabled || !removable} onclick={onRemove}>移除此 Peer</Button>
</fieldset>
