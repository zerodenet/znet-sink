<script lang="ts">
  import { onMount } from 'svelte';
  import { Clipboard, RefreshCcw } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import type { ModuleReport, ModuleStatus } from '$lib/features/diagnostics/model';
  let { load = () => import('$lib/services/module-diagnostics').then((service) => service.getModuleDiagnostics()) }: {load?: () => Promise<ModuleReport>} = $props();
  let report = $state<ModuleReport | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let feedback = $state('');
  let generation = 0;
  const labels: Record<ModuleStatus['state'], string> = {idle: '空闲／未加载', busy: '进行中', ready: '已读取', error: '异常', unavailable: '未知'};
  async function refresh() {
    if (loading) return;
    const current = generation;
    loading = true; error = null;
    try { const snapshot = await load(); if (current === generation) report = snapshot; }
    catch (cause) { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (current === generation) loading = false; }
  }
  async function copy() {
    if (!report) return;
    try { await copyTextToClipboard(JSON.stringify({schemaId: 'znet.modules.v1', ...report}, null, 2)); feedback = '已复制模块诊断'; }
    catch { feedback = '复制失败，请重试'; }
  }
  onMount(() => { void refresh(); return () => { generation++; }; });
</script>
<section class="module-panel" aria-label="模块状态">
  <div class="module-toolbar">
    <div><h2>模块状态</h2><p>读取当前状态不会启停内核、更改配置或触发网络探测。</p></div>
    <div class="actions"><Button variant="outline" size="sm" onclick={copy} disabled={!report}><Clipboard />复制诊断</Button><Button size="sm" onclick={refresh} disabled={loading}><RefreshCcw />{loading ? '读取中…' : '刷新'}</Button></div>
  </div>
  {#if error}<p role="alert">读取失败：{error}{report ? '；仍显示上次快照。' : ''}</p>{/if}
  {#if feedback}<p role="status">{feedback}</p>{/if}
  {#if report}
    <p class="stamp">采集时间：{new Date(report.collectedAt).toLocaleString()} · 最近错误保留用于排查，不代表当前仍故障</p>
    <div class="module-grid">
      {#each report.modules as module (module.id)}
        <article class="module-card">
          <header><h3>{module.title}</h3><span class:failure={module.state === 'error' || module.state === 'unavailable'}>{labels[module.state]}</span></header>
          <p>{module.summary}</p>
          {#if module.updatedAt}<p class="stamp">观测时间：{new Date(module.updatedAt).toLocaleString()}</p>{/if}
          <dl>{#each module.facts as fact}<div><dt>{fact.label}</dt><dd>{fact.value}</dd></div>{/each}</dl>
          {#if module.error}<p class="module-error">最近错误：{module.error}</p>{/if}
        </article>
      {/each}
    </div>
  {:else if loading}<p role="status">正在读取模块状态…</p>{/if}
</section>
<style>
  .module-panel { overflow: auto; padding: 4px 2px 16px; min-height: 0; }
  .module-toolbar, .actions, header { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .module-toolbar { flex-wrap: wrap; margin-bottom: 14px; } .actions { flex-wrap: wrap; }
  h2, h3, p { margin: 0; } h2 { font-size: 15px; } h3 { font-size: 13px; }
  p, dt, dd { font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; } p { margin-top: 6px; }
  .stamp, dt, .module-toolbar p { color: var(--muted-foreground); }
  .module-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 280px), 1fr)); gap: 12px; margin-top: 12px; }
  .module-card { border: 1px solid var(--border); border-radius: 10px; padding: 14px; min-width: 0; }
  header span { font-size: 11px; border: 1px solid var(--border); border-radius: 5px; padding: 2px 6px; white-space: nowrap; }
  .failure, .module-error { color: var(--destructive); } dl { margin: 10px 0 0; } dl div { display: grid; grid-template-columns: minmax(70px, 1fr) 2fr; gap: 8px; } dd { margin: 0; }
</style>
