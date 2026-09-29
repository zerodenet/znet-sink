<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';
  let { node }: { node: ProxyNode } = $props();
  const endpoint = $derived(node.server && node.port
    ? `${node.server.includes(':') ? `[${node.server}]` : node.server}:${node.port}`
    : undefined);
</script>

{#if node.protocol.toLowerCase() === 'wireguard'}
  <span class="wireguard-endpoint" title={endpoint ?? '多 Peer 或无固定地址的端点，请查看配置中的 peers'}>
    {endpoint ?? '查看配置中的 Peer 端点'}
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
