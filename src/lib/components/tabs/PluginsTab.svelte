<script lang="ts">
  import PluginCatalog from './PluginCatalog.svelte';
  import './plugins.css';
  import { Puzzle, Upload, RefreshCw, ShieldCheck, Play, Square, Trash2, Search } from '@lucide/svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Input } from '$lib/components/ui/input';
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Choice } from '$lib/components/ui/choice';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { getAppErrorInfo } from '$lib/services/core';
  import { pluginApi, type PluginComponent, type PluginSnapshot } from '$lib/services/plugins';
  import { canApprove, selectedGrants, permissionKey, permissionLabel } from '$lib/services/plugin-permissions';

  let installedQuery = $state('');
  let section = $state<'discover' | 'installed'>('discover');
  let supported = $state<boolean | null>(null);
  let snapshot = $state<PluginSnapshot>({ checked: false, components: [], notices: [] });
  let busy = $state(false);
  let runningKey = $state<string | null>(null);
  let message = $state('');
  let error = $state('');
  let result = $state('');
  let reviewOpen = $state(false);
  let selectedComponent = $state<PluginComponent | null>(null);
  let selected = $state<string[]>([]);
  let removeOpen = $state(false);
  let removeComponent = $state<PluginComponent | null>(null);
  const installedComponents = $derived(snapshot.components.filter(component =>
    `${component.name} ${component.plugin_id} ${component.component_id} ${component.publisher}`.toLowerCase().includes(installedQuery.trim().toLowerCase())));
  let runGeneration = 0;
  let alive = true;

  async function reload() {
    const next = await pluginApi.snapshot();
    if (alive) snapshot = next;
  }
  onMount(() => {
    alive = true;
    void pluginApi.supported().then(async value => { if (!alive) return; supported = value; if (value) await reload(); })
      .catch(reason => { error = getAppErrorInfo(reason, '无法读取插件状态').message; });
    const timer = setInterval(() => { if (supported) void reload().catch(() => {}); }, 3000);
    return () => { alive = false; runGeneration++; clearInterval(timer); };
  });
  async function action(work: () => Promise<PluginSnapshot>, success: string) {
    busy = true; error = ''; message = ''; result = '';
    try { snapshot = await work(); message = success; }
    catch (reason) { error = getAppErrorInfo(reason, '插件操作失败').message; await reload().catch(() => {}); }
    finally { busy = false; }
  }
  async function install() {
    try {
      const path = await open({ title: '选择插件包', multiple: false, directory: false, filters: [{ name: 'ZNet Sink 插件', extensions: ['zspkg'] }] });
      if (typeof path === 'string') { section = 'installed'; await action(() => pluginApi.install(path), '已安装，请查看权限后决定是否启用'); }
    } catch (reason) { error = getAppErrorInfo(reason, '无法选择插件包').message; }
  }
  function review(component: PluginComponent) {
    selectedComponent = $state.snapshot(component);
    selected = component.permissions.filter(p => p.supported && p.granted).map(p => permissionKey(p.request));
    reviewOpen = true;
  }
  async function approve() {
    const component = selectedComponent;
    if (!component?.review || !canApprove(component, selected)) return;
    const grants = selectedGrants(component, selected);
    reviewOpen = false;
    await action(() => pluginApi.authorize(component.review!, grants), '已授权，可运行此组件');
  }
  async function run(component: PluginComponent) {
    if (!component.review) return;
    const generation = ++runGeneration;
    runningKey = component.review.key; error = ''; message = ''; result = '';
    try {
      const value = await pluginApi.run(component.review);
      if (generation === runGeneration && alive) { result = JSON.stringify(value, null, 2); message = '运行完成'; }
    } catch (reason) {
      if (generation === runGeneration && alive) error = getAppErrorInfo(reason, '运行失败').message;
    } finally { if (alive) { runningKey = null; await reload().catch(() => {}); } }
  }
  async function stop(component: PluginComponent) {
    if (!component.review) return;
    ++runGeneration;
    try { snapshot = await pluginApi.stop(component.review.key); result = ''; message = '权限已撤销'; error = ''; }
    catch (reason) { error = getAppErrorInfo(reason, '停用失败').message; }
  }
</script>

{#snippet sections()}
  <SegmentedControl.Root value={section} onValueChange={(value) => { section = value as typeof section; }} aria-label="插件页面" disabled={busy}>
    <SegmentedControl.Item value="discover">发现插件</SegmentedControl.Item>
    <SegmentedControl.Item value="installed">已安装</SegmentedControl.Item>
  </SegmentedControl.Root>
{/snippet}

<div class="desk-card plugins-panel animate-fade-in">
  <div class="plugins-header">
    <div class="plugins-title-group"><h2>插件管理</h2><p>发现与安装插件，管理访问权限</p></div>
    {#if supported}
      <Button variant="outline" size="sm" disabled={busy || runningKey !== null} onclick={install}><Upload size={14} />从本地安装</Button>
    {/if}
  </div>
  {#if supported === null}<div class="plugins-empty" role="status">正在读取插件状态…</div>
  {:else if !supported}<div class="plugins-empty"><Puzzle size={28} />当前设备尚未开放插件运行。</div>
  {:else}
    {#if section === 'discover'}
      <PluginCatalog navigation={sections} disabled={busy || runningKey !== null} oninstalled={(value) => { snapshot = value; section = 'installed'; message = '已安装，请查看权限后决定是否启用'; error = ''; }} />
    {:else}
      <div class="plugins-toolbar">
        {@render sections()}
        <div class="plugins-toolbar-actions">
          <div class="plugins-search"><Search size={14} aria-hidden="true" /><Input class="pl-8" aria-label="搜索已安装插件" placeholder="搜索已安装插件…" bind:value={installedQuery} /></div>
          <Button variant="outline" size="sm" disabled={busy || runningKey !== null} onclick={() => action(pluginApi.refresh, '已核验中央登记与已安装插件')}><RefreshCw size={14} class={busy ? 'animate-spin' : ''} />检查已安装插件</Button>
        </div>
      </div>
      <div class="plugins-scroll">
        <p class="plugins-hint">安装后默认不授予权限。客户端重启后需要重新授权。</p>
        {#if !snapshot.checked}<p class="plugins-notice">请先检查插件登记，再确认访问权限。</p>{/if}
        {#each snapshot.notices as notice}<p class="plugins-notice">{notice}</p>{/each}
        {#if snapshot.checked && snapshot.components.length === 0}
          <div class="plugins-empty"><Puzzle size={28} /><strong>还没有已安装的插件</strong><p>在线发现插件，或从本地选择插件包。</p><Button variant="outline" size="sm" onclick={() => { section = 'discover'; }}>发现插件</Button></div>
        {:else if snapshot.components.length > 0 && !installedComponents.length}
          <div class="plugins-empty"><Search size={28} /><p>没有找到匹配的插件。</p><Button variant="ghost" size="sm" onclick={() => { installedQuery = ''; }}>清除搜索</Button></div>
        {:else}
          <div class="plugins-grid">
            {#each installedComponents as component (`${component.plugin_id}/${component.component_id}`)}
              <article class="plugin-card">
                <div class="plugin-card-heading">
                  <div class="plugin-icon"><Puzzle size={18} /></div>
                  <div class="plugin-identity"><strong>{component.name}</strong><span class="plugin-meta">发布者 {component.publisher}</span></div>
                  <span class="plugin-version">{component.version}</span>
                </div>
                <div class="plugin-details"><span class="plugin-meta">{component.component_id}</span><span class="plugin-status" class:authorized={component.enabled && !component.blocked} class:blocked={!!component.blocked}>{component.blocked ? '暂不可用' : component.running || runningKey === component.review?.key ? '运行中' : component.enabled ? '已授权' : '未授权'}</span></div>
                {#if component.blocked}<p class="plugins-error">{component.blocked}</p>{/if}
                <div class="plugin-card-actions">
                  <Button size="sm" variant="outline" disabled={busy || runningKey !== null || !snapshot.checked || !component.review || !!component.blocked} onclick={() => review(component)}><ShieldCheck size={14} />查看权限</Button>
                  <Button size="sm" disabled={busy || runningKey !== null || !snapshot.checked || !component.enabled || !!component.blocked} onclick={() => run(component)}><Play size={14} />运行</Button>
                  {#if component.enabled || component.running || runningKey === component.review?.key}
                    <Button size="sm" variant="outline" onclick={() => stop(component)}><Square size={14} />停用</Button>
                  {/if}
                  <Button size="sm" variant="ghost" class="text-destructive hover:text-destructive" disabled={busy || runningKey !== null} onclick={() => { removeComponent = component; removeOpen = true; }}><Trash2 size={14} />卸载</Button>
                </div>
                {#if component.blocked}
                  {#each component.permissions as permission}<p class="plugin-meta">{permissionLabel(permission.request.capability)} · {permission.request.scope}{permission.supported ? '' : ' · 暂不可用'}</p>{/each}
                {/if}
              </article>
            {/each}
          </div>
        {/if}
        {#if result}<details class="plugins-result" open><summary>运行结果</summary><pre>{result}</pre></details>{/if}
      </div>
    {/if}
  {/if}
  {#if message || error}<div class="plugins-feedback" aria-live="polite">{#if message}<p>{message}</p>{/if}{#if error}<p class="plugins-error" role="alert">{error}</p>{/if}</div>{/if}
</div>

<Dialog.Root bind:open={reviewOpen}>
  <Dialog.Content class="sm:max-w-[480px]">
    <Dialog.Header><Dialog.Title>插件权限</Dialog.Title><Dialog.Description>{selectedComponent?.name} 申请以下权限。仅勾选你愿意允许的访问范围。</Dialog.Description></Dialog.Header>
    {#if selectedComponent}
      <Dialog.Body class="space-y-4">
      <p class="plugin-meta">{selectedComponent.version} · 发布者 {selectedComponent.publisher}</p>
      {#each selectedComponent.permissions as permission}
        <label class="permission-row">
          <Choice
            checked={selected.includes(permissionKey(permission.request))}
            onchange={(event) => {
              const key = permissionKey(permission.request);
              selected = event.currentTarget.checked ? [...selected.filter(value => value !== key), key] : selected.filter(value => value !== key);
            }}
            disabled={!permission.supported}
          />
          <span>{permissionLabel(permission.request.capability)}{permission.required ? '（必需）' : '（可选）'}<small>{permission.request.scope === 'self' ? '仅此插件的身份信息' : permission.request.scope}{permission.supported ? '' : ' · 暂不可用'}</small></span>
        </label>
      {/each}
      {#if selectedComponent.permissions.length === 0}<p>此组件未申请宿主访问权限。</p>{/if}
      <p class="plugin-meta">本次授权最多有效 10 分钟，客户端退出后失效。可随时停用。</p>
      </Dialog.Body>
      <Dialog.Footer><Button variant="outline" onclick={() => { reviewOpen = false; }}>取消</Button><Button disabled={!canApprove(selectedComponent, selected)} onclick={approve}>允许并启用</Button></Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>
<Dialog.Root bind:open={removeOpen}>
  <Dialog.Content class="sm:max-w-[420px]">
    <Dialog.Header><Dialog.Title>卸载插件</Dialog.Title><Dialog.Description>卸载 {removeComponent?.name} 的全部组件并撤销权限？</Dialog.Description></Dialog.Header>
    <Dialog.Footer><Button variant="outline" onclick={() => { removeOpen = false; }}>取消</Button><Button variant="destructive" onclick={() => { const id = removeComponent?.plugin_id; removeOpen = false; if (id) void action(() => pluginApi.uninstall(id), '插件已卸载'); }}>卸载</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .permission-row { display: flex; align-items: flex-start; gap: 10px; padding: 10px; border: 1px solid var(--border); border-radius: 8px; font-size: 12px; }
  .permission-row :global([data-slot='choice']) { margin-top: 3px; flex-shrink: 0; }
  .permission-row span { min-width: 0; }
  small { display: block; margin-top: 4px; color: var(--muted-foreground); font-size: 11px; overflow-wrap: anywhere; }
</style>
