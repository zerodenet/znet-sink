<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Textarea } from '$lib/components/ui/textarea';
  import * as Dialog from '$lib/components/ui/dialog';
  import { settingsTransferGateway } from '$lib/services/settings-transfer';
  import type { MigrationState } from '$lib/features/mobile/settings-migration';
  import { settingsMigrationSession as migration, subscribeSettingsMigration } from '$lib/services/settings-migration-session';
  import type { SettingsTransferExport } from '$lib/types/settings-transfer';
  import { getNativeDocumentCapabilities, pickNativeDocument, exportNativeSettings } from '$lib/services/native-documents';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import { store } from '$lib/services/store.svelte';
  import { applyTheme, type ThemeMode } from '$lib/services/theme.svelte';
  const navigation = useMobileNavigation();
  let migrationState = $state<MigrationState>({ content: '', phase: 'editing', preview: null, receipt: null, error: null });
  let draft = $state(migration.state.content);
  let savedDraft = $state(migration.state.receipt?.localState === 'saved' ? migration.state.content : '');
  let discard = $state(false);
  let pendingNavigation = $state<(() => void) | null>(null);
  let confirmation = $state(false);
  let exported = $state<SettingsTransferExport | null>(null);
  let ioStatus = $state<string | null>(null);
  let ioBusy = $state(false);
  let nativeAvailable = $state(false);
  let nativeReason = $state('正在读取系统文件能力…');
  const busy = $derived(ioBusy || ['previewing','saving','checking'].includes(migrationState.phase));
  const importBusy = $derived(busy || migrationState.phase === 'unknown');
  onMount(() => {
    let alive = true;
    const unsubscribeMigration = subscribeSettingsMigration(next => {
      migrationState = next;
      draft = next.content;
      if (next.phase === 'saved' && next.receipt?.localState === 'saved') savedDraft = next.content;
      if (!next.preview) confirmation = false;
    });
    void getNativeDocumentCapabilities().then(capabilities => {
      if (!alive) return;
      nativeAvailable = capabilities.available && (capabilities.platform === 'android' || capabilities.platform === 'ios');
      nativeReason = capabilities.reason ?? '系统文件选择、保存和分享按设备能力开放。';
    }).catch(() => { if (alive) nativeReason = '系统文件适配尚不可用；仍可粘贴 JSON 校验和预览。'; });
    return () => { alive = false; unsubscribeMigration(); };
  });
  $effect(() => navigation.guardBeforeLeave(continueNavigation => {
    if (busy) return false;
    if (draft.trim() && draft !== savedDraft) {
      pendingNavigation = continueNavigation; discard = true; return false;
    }
    return true;
  }));
  function leaveDraft() {
    discard = false;
    if (migrationState.phase !== 'unknown') { draft = ''; migration.edit(''); }
    savedDraft = draft;
    const resume = pendingNavigation; pendingNavigation = null; resume?.();
  }
  function edit() { migration.edit(draft); }
  async function confirm() {
    confirmation = false;
    await migration.confirm();
    if (migrationState.receipt?.localState === 'saved') savedDraft = draft;
    if (migrationState.receipt?.localState === 'saved' && ['system','light','dark'].includes(migrationState.receipt.theme)) {
      store.selectedTheme = migrationState.receipt.theme as ThemeMode;
      applyTheme(store.selectedTheme);
    }
  }
  async function pick() {
    if (busy) return;
    ioBusy = true; ioStatus = null;
    try {
      const receipt = await pickNativeDocument('settings');
      if (receipt.status === 'read' && typeof receipt.content === 'string') { draft = receipt.content; migration.edit(draft); ioStatus = '文件已读取，尚未导入；请校验并预览。'; }
      else if (receipt.status === 'cancelled') ioStatus = '已取消文件选择，草稿保留。';
      else ioStatus = receipt.reason ?? '文件读取未完成，草稿保留。';
    } catch { ioStatus = '文件读取失败，草稿保留。'; }
    finally { ioBusy = false; }
  }
  async function prepareExport() {
    if (busy) return;
    ioBusy = true; ioStatus = null;
    try { exported = await settingsTransferGateway.export(); }
    catch { ioStatus = '导出范围读取失败；未生成或保存文件。'; }
    finally { ioBusy = false; }
  }
  async function publish(delivery: 'save' | 'share') {
    if (busy || !exported) return;
    ioBusy = true; ioStatus = null;
    try {
      const receipt = await exportNativeSettings(delivery, exported.expectedRevision);
      ioStatus = receipt.status === 'saved' ? '系统文件保存已确认。' : receipt.status === 'share-accepted' ? '系统分享已接受；不代表接收方已保存。' : receipt.status === 'cancelled' ? '已取消，设置与导出预览保留。' : receipt.reason ?? '系统操作结果未确认，请重新核对。';
    } catch { ioStatus = '系统操作结果未确认，请重新核对；不会自动重试。'; }
    finally { ioBusy = false; }
  }
</script>
<section class="migration" aria-label="设置迁移">
  <p>迁移客户端显式设置及外观、日志偏好。订阅正文、代理凭据、插件秘密和授权、系统 VPN 授权、桌面路径、接管开关及发行渠道/自动检查意图均不导入或导出。</p>
  <p class="hint">迁移采用保守范围：DNS 仅接受系统、IP 或内置公开解析器的精确主机名，TLS 名称也受约束；自定义私有上游与非标准路径整体排除。测速 URL 仅接受根路径及内置公共测试路径。原设置和高级 DNS 编辑能力保留。</p>
  <h2>导入</h2>
  <Button variant="outline" disabled={importBusy || !nativeAvailable} onclick={pick}>从系统文件读取</Button>
  <p class="hint">{nativeReason}</p>
  <label for="settings-transfer-content">粘贴迁移 JSON</label>
  <Textarea id="settings-transfer-content" aria-label="粘贴迁移 JSON" bind:value={draft} oninput={edit} disabled={importBusy} spellcheck="false" rows={9} />
  <Button disabled={importBusy || !draft.trim()} onclick={() => migration.preview()}>{migrationState.phase === 'previewing' ? '正在校验…' : '校验并预览'}</Button>
  {#if migrationState.preview}
    <article class="preview" aria-label="固定导入范围">
      <h3>将覆盖 {migrationState.preview.changedFields.length} 项</h3>
      <ul>{#each migrationState.preview.changedFields as field}<li>{field}</li>{:else}<li>没有字段变化</li>{/each}</ul>
      <p>预览所属配置：{migrationState.preview.scope.profileId ?? '无配置'}；来源版本：{migrationState.preview.sourceRevision ?? '未提供'}；设置版本：{migrationState.preview.expectedRevision.slice(0,12)}</p>
      <p>{migrationState.preview.notice}</p>
      {#if migrationState.preview.runtimeRequired}<p>原生配置结构已校验；运行能力、已编译协议与平台支持仍须在运行应用前预检。</p>{/if}
      {#if migrationState.preview.compatibilityPending}<p>无当前来源配置，协议与出口兼容性待选择配置后预检。</p>{/if}
      {#if migrationState.phase === 'ready'}<Button disabled={busy} onclick={() => confirmation = true}>确认导入范围</Button>{/if}
    </article>
  {/if}
  {#if migrationState.phase === 'saving'}<p role="status">正在原子保存本地设置…</p>{/if}
  {#if migrationState.error}<p role="alert" class="error">{migrationState.error}</p>{/if}
  {#if migrationState.receipt}<p role="status">{migrationState.receipt.notice}</p>{/if}
  {#if migrationState.phase === 'unknown'}<Button variant="outline" onclick={() => migration.check()}>核对原操作</Button>{/if}
  {#if migrationState.receipt?.runtimeState === 'pending-apply'}<Button variant="outline" onclick={() => navigation.open('workspace')}>进入工作区预检运行应用</Button>{/if}
  <h2>导出</h2>
  <Button variant="outline" disabled={busy} onclick={prepareExport}>预览可迁移设置</Button>
  {#if exported}
    <p>导出仅包含允许迁移的字段。{exported.omittedFields.length ? `已整体排除：${exported.omittedFields.join('、')}。` : ''}</p>
    {#if exported.omittedFields.length}<p>上述字段含敏感材料或不符合当前迁移 schema，因此整块排除；原设置未改变。</p>{/if}
    <Textarea readonly aria-label="可迁移设置 JSON" value={exported.document} rows={7} />
    <div class="actions"><Button variant="outline" disabled={busy || !nativeAvailable} onclick={() => publish('save')}>系统保存</Button><Button variant="outline" disabled={busy || !nativeAvailable} onclick={() => publish('share')}>系统分享</Button></div>
  {/if}
  {#if ioStatus}<p role="status">{ioStatus}</p>{/if}
</section>
<Dialog.Root bind:open={confirmation}><Dialog.Content><Dialog.Header><Dialog.Title>导入已预览的设置范围？</Dialog.Title><Dialog.Description>确认时核对原设置版本、所属配置和实例。仅保存本地设置；运行生效需另行预检，不会自动开启 VPN。</Dialog.Description></Dialog.Header><Dialog.Footer><Button class="min-h-11" variant="outline" onclick={() => confirmation = false}>继续编辑</Button><Button class="min-h-11" onclick={confirm}>确认本地导入</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<Dialog.Root bind:open={discard}><Dialog.Content><Dialog.Header><Dialog.Title>离开未保存的迁移草稿？</Dialog.Title><Dialog.Description>{migrationState.phase === 'unknown' ? '原保存结果未确认。离开会保留原操作 ID 和核对入口，返回后不会重放。进程重建仍需核对本地设置。' : '继续编辑会保留草稿；放弃只清除本页草稿，不修改已保存设置。'}</Dialog.Description></Dialog.Header><Dialog.Footer><Button class="min-h-11" variant="outline" onclick={() => { discard = false; pendingNavigation = null; }}>继续编辑</Button><Button class="min-h-11" variant="destructive" onclick={leaveDraft}>{migrationState.phase === 'unknown' ? '保留原操作并离开' : '放弃草稿'}</Button></Dialog.Footer></Dialog.Content></Dialog.Root>
<style>
  .migration { padding:16px 14px 26px; overflow:auto; min-width:0; }
  h2 { margin:20px 0 10px; font-size:1rem; font-weight:500; } h3 { margin:0 0 8px; font-size:1rem; }
  p { font-size:.85rem; color:var(--muted-foreground); line-height:1.6; overflow-wrap:anywhere; }
  label { display:block; margin:14px 0 8px; }
  .migration :global(button) { min-height:44px; margin:8px 0; }
  .migration :global(textarea) { font-size:1rem; min-height:140px; width:100%; }
  .preview { padding:12px; margin-top:14px; border:1px solid var(--border); border-radius:12px; background:var(--card); }
  ul { font-size:.85rem; list-style:disc; padding-left:20px; overflow-wrap:anywhere; }
  .error { color:var(--destructive); }.actions { display:flex; flex-wrap:wrap; gap:8px; }
</style>
