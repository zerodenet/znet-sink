<script lang="ts">
  import DnsPanel from '../../src/lib/features/dns/Panel.svelte';
  import RoutePanel from '../../src/lib/features/routing/Panel.svelte';
  const route = new URLSearchParams(location.search).get('tool') === 'route';
</script>
<main><p class="fixture-note">真实生产组件；IO 为测试夹具，不启动内核或 VPN。</p>{#if route}<RoutePanel />{:else}<DnsPanel />{/if}</main>
<style>main{padding:12px;max-width:720px;margin:auto;min-width:0;}.fixture-note{font-size:11px;color:var(--muted-foreground);margin-bottom:12px;overflow-wrap:anywhere;}</style>
