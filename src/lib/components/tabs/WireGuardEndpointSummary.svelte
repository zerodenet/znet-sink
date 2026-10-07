<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';
  let { node }: { node: ProxyNode } = $props();
  const addresses = $derived((node.localAddresses ?? []).filter((address) => address.trim()).join(' · '));
</script>

{#if node.protocol.toLowerCase() === 'wireguard'}
  <span class="wireguard-endpoint" title={addresses ? `本机隧道 IP（配置值）：${addresses}` : '配置中未提供本机隧道地址'}>
    本机 IP {addresses || '—'}
  </span>
{/if}

<style>
  .wireguard-endpoint {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    line-height: 1.5;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
  }
</style>
