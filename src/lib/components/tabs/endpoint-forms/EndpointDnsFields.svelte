<script lang="ts">
  import { untrack } from 'svelte';
  import { Switch } from '$lib/components/ui/switch';
  import WireGuardField from './WireGuardField.svelte';
  import type { UserEndpoint } from '$lib/features/endpoints/library';
  import { parseAddressList } from '$lib/features/endpoints/protocols/wireguard';
  let { endpoint = $bindable(), disabled, attempted, error, allowDefaultChoice = true }: { endpoint: UserEndpoint; disabled: boolean; attempted: boolean; error: (field: string) => string; allowDefaultChoice?: boolean } = $props();
  const id = $props.id();
  let servers = $state(untrack(() => endpoint.dns?.servers.join(', ') ?? ''));
  let domains = $state(untrack(() => endpoint.dns?.search_domains.join(', ') ?? ''));
  function change() {
    if (!servers.trim() && !domains.trim()) { delete endpoint.dns; return; }
    endpoint.dns = { servers: parseAddressList(servers), search_domains: parseAddressList(domains), use_as_default: endpoint.dns?.use_as_default ?? true };
  }
</script>
<fieldset class="grid min-w-0 gap-3 rounded-lg border border-border p-4" {disabled}>
  <legend class="px-1 text-xs font-medium">DNS</legend>
  <WireGuardField label="DNS 服务器" name="dns_servers" value={servers} onValueChange={value => { servers = value; change(); }} {disabled} {attempted} error={error('dns_servers')} placeholder="10.66.0.1, 1.1.1.1" hint="对应文件的 DNS。保存时一起应用；按 AllowedIPs 范围选择经端点或直连。" />
  <WireGuardField label="DNS 搜索域" name="dns_search_domains" value={domains} onValueChange={value => { domains = value; change(); }} {disabled} {attempted} error={error('dns_search_domains')} hint="保留配置中的搜索域；系统是否补全短主机名取决于系统设置。" />
  {#if endpoint.dns && allowDefaultChoice}<div class="flex items-center gap-2 text-xs"><span id={id}>用作默认 DNS</span><Switch aria-labelledby={id} checked={endpoint.dns.use_as_default} onCheckedChange={value => { if (endpoint.dns) endpoint.dns.use_as_default = value; }} {disabled}/></div>{/if}
  <p class="text-xs text-muted-foreground">保留原有 DNS 规则。停用端点或关闭出站后恢复原默认解析；再次启用时自动恢复此 DNS。</p>
</fieldset>
