<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Switch } from '$lib/components/ui/switch';
  import { getAppConfig, updateAppConfig, getAppErrorMessage } from '$lib/services/core';
  import type { AppConfig, ConfigOverrides } from '$lib/types/app-config';
  let { onApplied = () => {} }: {onApplied?: () => void} = $props();
  let config = $state<AppConfig | null>(null);
  let overrides = $state<ConfigOverrides>({listener:false,dns:false,tun:false,urlTest:false,bypass:false,rules:false});
  let busy = $state(false);
  let error = $state('');
  let saved = $state(false);
  const capabilities: {key:keyof ConfigOverrides; label:string}[] = [
    {key:'listener',label:'代理入口'},{key:'urlTest',label:'公共测速'},
    {key:'dns',label:'DNS'},{key:'tun',label:'TUN 参数'},
    {key:'bypass',label:'绕过规则'},{key:'rules',label:'通用规则追加'},
  ];
  async function refresh() {
    busy = true; error = '';
    try { config = await getAppConfig(); if (config.overrides) overrides = {...config.overrides}; }
    catch (e) { error = getAppErrorMessage(e, '读取配置来源失败'); }
    finally { busy = false; }
  }
  async function save() {
    busy = true; error = ''; saved = false;
    try { await updateAppConfig({overrides}); await refresh(); if (!error) { saved = true; onApplied(); } }
    catch (e) { error = getAppErrorMessage(e, '保存优先级失败'); }
    finally { busy = false; }
  }
  onMount(() => { void refresh(); });
</script>

<section aria-label="配置优先级" class="precedence">
  <h3>配置优先级</h3>
  <p>默认保留当前配置中的设置，缺失时才使用客户端缺省值。需要替换时，单独开启“客户端覆盖”。更改优先级可能重启内核并重建已开启的 TUN。</p>
  <p>下方是按已保存设置解析的结果；内核运行状态请在概览或调试页查看。</p>
  {#if config}
    <div class="choices">
      {#each capabilities as capability}
        {@const resolved = config.resolved?.find(row => row.key === capability.key)}
        <div class="choice">
          <div><strong>{capability.label}</strong><span>{resolved?.source ?? '配置优先'}</span><p>{resolved?.value ?? '来源信息暂不可用'}</p></div>
          <label><span>客户端覆盖</span><Switch bind:checked={overrides[capability.key]} disabled={busy} onCheckedChange={() => saved = false} aria-label={`${capability.label}客户端覆盖`} /></label>
        </div>
      {/each}
    </div>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="actions"><Button size="sm" variant="outline" onclick={refresh} disabled={busy}>刷新来源</Button><Button size="sm" onclick={save} disabled={busy || !config}>{busy ? '处理中…' : saved ? '已保存' : '保存优先级'}</Button></div>
</section>

<style>
  .precedence { margin-bottom: 20px; padding-bottom: 16px; border-bottom: 1px solid var(--border); }
  h3 { font-size: 14px; font-weight: 600; margin: 0 0 8px; }
  p { font-size: 12px; color: var(--muted-foreground); line-height: 1.6; margin: 4px 0; overflow-wrap: anywhere; }
  .choices { margin-block: 12px; }
  .choice { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 0; border-bottom: 1px solid var(--border); }
  .choice > div { min-width: 0; }
  strong { font-size: 13px; font-weight: 500; }
  .choice > div > span { font-size: 11px; color: var(--muted-foreground); margin-left: 10px; }
  label { display: flex; align-items: center; gap: 8px; flex-shrink: 0; font-size: 12px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  [role="alert"] { color: var(--destructive); }
  @media (max-width: 560px) { .choice { align-items: flex-start; flex-direction: column; gap: 8px; } }
</style>
