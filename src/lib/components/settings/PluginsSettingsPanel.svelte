<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { getAppErrorInfo } from '$lib/services/core';
  import { pluginApi, type PluginComponent, type PluginSnapshot } from '$lib/services/plugins';
  import { canApprove, selectedGrants, permissionKey, permissionLabel } from '$lib/services/plugin-permissions';

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
      if (typeof path === 'string') await action(() => pluginApi.install(path), '已安装，请查看权限后决定是否启用');
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

<div class="plugins-panel">
  <div class="heading"><div><h2>插件</h2><p>管理已安装的插件及其访问权限。</p></div></div>
  {#if supported === null}<p class="muted">正在读取插件状态…</p>
  {:else if !supported}<p class="muted">当前设备尚未开放插件运行。</p>
  {:else}
    <div class="actions">
      <Button variant="outline" size="sm" disabled={busy || runningKey !== null} onclick={() => action(pluginApi.refresh, '已核验中央登记与已安装插件')}>检查已安装插件</Button>
      <Button variant="outline" size="sm" disabled={busy || runningKey !== null} onclick={install}>安装插件包</Button>
    </div>
    <p class="muted">安装包须通过中央登记与发布者签名校验。安装后默认不授予权限。</p>
    {#if !snapshot.checked}<p class="hint">请先检查插件登记。客户端重启后需要重新授权。</p>{/if}
    {#if snapshot.checked && snapshot.components.length === 0}<div class="empty">还没有可加载的插件。可从发布者获取已登记的插件包后安装。</div>{/if}
    {#each snapshot.notices as notice}<p class="hint">{notice}</p>{/each}
    {#each snapshot.components as component (`${component.plugin_id}/${component.component_id}`)}
      <article class="plugin-card">
        <div class="card-heading"><strong>{component.name}</strong><span class="muted">{component.version}</span></div>
        <p class="muted">{component.component_id} · 发布者 {component.publisher}</p>
        <p>{component.blocked ?? (component.running || runningKey === component.review?.key ? '运行中' : component.enabled ? '已授权' : '未授权')}</p>
        <div class="actions">
          <Button size="sm" variant="outline" disabled={busy || runningKey !== null || !snapshot.checked || !component.review || !!component.blocked} onclick={() => review(component)}>查看权限</Button>
          <Button size="sm" disabled={busy || runningKey !== null || !snapshot.checked || !component.enabled || !!component.blocked} onclick={() => run(component)}>运行</Button>
          {#if component.enabled || component.running || runningKey === component.review?.key}
            <Button size="sm" variant="outline" onclick={() => stop(component)}>停用</Button>
          {/if}
          <Button size="sm" variant="ghost" disabled={busy || runningKey !== null} onclick={() => { removeComponent = component; removeOpen = true; }}>卸载</Button>
        </div>
        {#if component.blocked}
          {#each component.permissions as permission}<p class="muted">{permissionLabel(permission.request.capability)} · {permission.request.scope}{permission.supported ? '' : ' · 暂不可用'}</p>{/each}
        {/if}
      </article>
    {/each}
  {/if}
  <div aria-live="polite">{#if message}<p>{message}</p>{/if}{#if error}<p class="error">{error}</p>{/if}</div>
  {#if result}<details open><summary>运行结果</summary><pre>{result}</pre></details>{/if}
</div>

<Dialog.Root bind:open={reviewOpen}>
  <Dialog.Content class="sm:max-w-[480px]">
    <Dialog.Header><Dialog.Title>插件权限</Dialog.Title><Dialog.Description>{selectedComponent?.name} 申请以下权限。仅勾选你愿意允许的访问范围。</Dialog.Description></Dialog.Header>
    {#if selectedComponent}
      <Dialog.Body>
      <p class="muted">{selectedComponent.version} · 发布者 {selectedComponent.publisher}</p>
      {#each selectedComponent.permissions as permission}
        <label class="permission-row">
          <input type="checkbox" value={permissionKey(permission.request)} bind:group={selected} disabled={!permission.supported} />
          <span>{permissionLabel(permission.request.capability)}{permission.required ? '（必需）' : '（可选）'}<small>{permission.request.scope === 'self' ? '仅此插件的身份信息' : permission.request.scope}{permission.supported ? '' : ' · 暂不可用'}</small></span>
        </label>
      {/each}
      {#if selectedComponent.permissions.length === 0}<p>此组件未申请宿主访问权限。</p>{/if}
      <p class="muted">本次授权最多有效 10 分钟，客户端退出后失效。可随时停用。</p>
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
  .plugins-panel { padding: 24px; overflow: auto; width: 100%; min-width: 0; display: flex; flex-direction: column; gap: 16px; font-size: 13px; }
  h2 { font-size: 18px; font-weight: 600; margin-bottom: 6px; }
  p { margin: 0; overflow-wrap: anywhere; }
  .muted, small { color: var(--muted-foreground); font-size: 12px; }
  .actions, .card-heading { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .plugin-card { padding: 16px; border: 1px solid var(--border); border-radius: 10px; display: flex; flex-direction: column; gap: 10px; }
  .hint, .empty { padding: 14px; border-radius: 8px; background: var(--muted); color: var(--muted-foreground); }
  .error { color: var(--destructive); }
  .permission-row { display: flex; align-items: flex-start; gap: 10px; font-size: 13px; }
  .permission-row input { margin-top: 4px; }
  small { display: block; margin-top: 4px; overflow-wrap: anywhere; }
  pre { max-height: 240px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; padding: 12px; background: var(--muted); border-radius: 8px; }
  @media (max-width: 640px) { .plugins-panel { padding: 16px; } }
</style>
