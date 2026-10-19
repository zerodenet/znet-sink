<script lang="ts">
  import type { SubscriptionProfile } from '$lib/types/domain';
  import { pluginLabel, subscriptionUsedBytes } from '$lib/features/configuration/catalog';
  import { Button } from '$lib/components/ui/button';
  let { source, busy, canRemove, onupdate, onedit, onremove, onplugin }: {
    source: SubscriptionProfile; busy: boolean; canRemove: boolean;
    onupdate: () => void; onedit: () => void; onremove: () => void; onplugin: () => void;
  } = $props();
  const used = $derived(subscriptionUsedBytes(source));
  const ratio = $derived(source.totalBytes && used != null ? Math.min(100, used / source.totalBytes * 100) : null);
  function bytes(value: number | null | undefined): string {
    if (value == null) return '未知';
    const units = ['B', 'KB', 'MB', 'GB', 'TB']; let unit = 0;
    while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
    return `${unit ? value.toFixed(1) : value} ${units[unit]}`;
  }
  function date(value: number): string { return new Date(value).toLocaleString('zh-CN'); }
</script>
<section class="source" aria-label={`更新来源 ${source.name}`}>
  <div class="source-heading"><strong>{source.name}</strong><span class="source-badge">{source.managedSource ? `由 ${pluginLabel(source.managedSource.pluginId)} 托管` : 'URL 订阅'}</span></div>
  {#if source.managedSource}<div>来源: {source.managedSource.sourceName ?? source.managedSource.providerId}</div>{/if}
  <div class="metadata">{source.nodeCount == null ? '节点数未知' : `${source.nodeCount} 个节点`} · {source.managedSource ? '由插件更新' : !source.enabled ? '来源已停用' : source.updateIntervalSecs ? `每 ${source.updateIntervalSecs} 秒更新` : '手动更新'}</div>
  <div class="metadata" aria-label="来源同步时间">同步: {source.lastSyncAtUnixMs ? date(source.lastSyncAtUnixMs) : '尚未成功更新'}</div>
  {#if source.expireAtUnixMs != null}<div class="metadata">到期: {date(source.expireAtUnixMs)}（{source.expireAtUnixMs < Date.now() ? '已到期' : `剩 ${Math.ceil((source.expireAtUnixMs - Date.now()) / 86_400_000)} 天`}）</div>{/if}
  {#if used != null || source.totalBytes != null}
    <div class="usage">已用 {bytes(used)} / 总量 {bytes(source.totalBytes)}{#if ratio != null}<span>{ratio.toFixed(1)}%</span>{/if}</div>
    {#if ratio != null}<progress value={ratio} max="100" aria-label="订阅流量使用比例"></progress>{/if}
  {/if}
  {#if source.lastError}<p role="alert" class="source-error">上次更新失败：{source.lastError}；已保存的配置保持可用。</p>{/if}
  <div class="source-actions">
    {#if source.managedSource}<Button size="sm" variant="outline" onclick={onplugin}>前往来源插件</Button>
    {:else}
      <Button size="sm" variant="outline" disabled={busy || !source.enabled} onclick={onupdate}>更新来源</Button>
      <Button size="sm" variant="ghost" disabled={busy} onclick={onedit}>编辑来源</Button>
      <Button size="sm" variant="ghost" disabled={busy || !canRemove} onclick={onremove}>移除来源</Button>
    {/if}
  </div>
</section>
<style>
  .source { display:grid; gap:8px; padding:14px; border:1px solid var(--border); border-radius:10px; min-width:0; }
  .source-heading { display:flex; gap:8px; flex-wrap:wrap; align-items:baseline; overflow-wrap:anywhere; }
  .source-badge { color:var(--muted-foreground); font-size:11px; }
  .metadata { color:var(--muted-foreground); font-size:12px; overflow-wrap:anywhere; }
  .usage { display:flex; flex-wrap:wrap; gap:8px; font-size:12px; font-variant-numeric:tabular-nums; }
  progress { width:100%; height:5px; accent-color:var(--primary); }
  .source-actions { display:flex; gap:4px; flex-wrap:wrap; }
  .source-error { color:var(--destructive); font-size:12px; overflow-wrap:anywhere; }
</style>
