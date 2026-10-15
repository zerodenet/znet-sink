<script lang="ts">
  import { untrack, onMount } from 'svelte';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import { getNativeDocumentCapabilities, pickNativeDocument } from '$lib/services/native-documents';
  import NativeProfileExport from '$lib/components/mobile/NativeProfileExport.svelte';
  import ProfileRemovalDialog from '$lib/components/mobile/ProfileRemovalDialog.svelte';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import { ProfileEditor } from '$lib/features/profile-editor/state';
  import { Textarea } from '$lib/components/ui/textarea';
  import { open as openFile } from '@tauri-apps/plugin-dialog';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { getAppErrorMessage, handleAppError } from '$lib/services/core';
  import { listProxyConfigs, setActiveProxyConfig } from '$lib/services/config';
  import { getProfileEditorSnapshot, validateProfileEditor, saveProfileEditor, readProfileEditorFile } from '$lib/features/profile-editor/client';
  import type { ProxyConfigProfile } from '$lib/types/domain';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { AlertTriangle, FolderOpen, Plus, Search, Trash2 } from '@lucide/svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  let { mobile = false }: {mobile?:boolean} = $props();
  const mobileNavigation=untrack(()=>mobile ? useMobileNavigation() : null);
  const editor=new ProfileEditor({snapshot:getProfileEditorSnapshot,validate:validateProfileEditor,save:saveProfileEditor},value=>{editorView=value;},()=>{localNotice='原本地保存结果已返回，当前草稿未被替换；请核对本地版本。';});
  let editorView=$state(editor.view);
  let configs=$state<ProxyConfigProfile[]>([]),loading=$state(true),loadError=$state<string|null>(null);
  let activatingId=$state<string|null>(null),deleteTarget=$state<ProxyConfigProfile|null>(null),deleteOpen=$state(false);
  let query=$state(''),showEditor=$state(false),editingId=$state<string|null>(null),editorTitle=$state('新建代理配置');
  let selectedProfile=$state<ProxyConfigProfile|null>(null),discardEditor=$state(false),pendingNavigation=$state<(()=>void)|null>(null);
  let nativeFilesAvailable=$state(false),nativeFilesReason=$state('正在读取系统文件能力…'),localNotice=$state<string|null>(null);
  const nativeHost=$derived(mobilePlatform.runtimePlatform!=='desktop');
  const saving=$derived(editorView.phase==='saving');
  const dirty=$derived(editorView.name!==editorView.originalName || editorView.sourceJson!==editorView.originalJson);
  const canEdit=$derived(store.isActionOperable('proxyConfig.upsert'));
  const canRemove=$derived(store.isActionOperable('proxyConfig.remove'));
  const busy=$derived(saving || activatingId!==null || deleteOpen);
  const canValidate=$derived(canEdit && !!editorView.binding && !editorView.readOnlyReason && !['loading','saving','validating','unknown','error'].includes(editorView.phase));
  const canSave=$derived(canValidate && !editorView.readingFile && !!editorView.name.trim() && !!editorView.sourceJson.trim());
  const filteredConfigs=$derived(configs.filter(item=>[item.name,item.id,item.path??''].join(' ').toLowerCase().includes(query.trim().toLowerCase())));
  const activeProfile=$derived(configs.find(item=>item.active)??null);
  onMount(()=>{
    let alive=true;
    if(mobilePlatform.runtimePlatform!=='desktop')void getNativeDocumentCapabilities().then(value=>{if(alive){nativeFilesAvailable=value.available;nativeFilesReason=value.reason??'取消文件选择会保留草稿。';}}).catch(()=>{if(alive)nativeFilesReason='系统文件适配不可用，仍可粘贴 JSON。';});
    return()=>{alive=false;editor.dispose();};
  });
  async function refresh(){loading=true;loadError=null;try{configs=await listProxyConfigs();}catch(error){loadError=getAppErrorMessage(error,'加载代理配置失败');}finally{loading=false;}}
  function openCreate(){if(busy)return;editingId=null;selectedProfile=null;editorTitle='新建代理配置';discardEditor=false;showEditor=true;void editor.open();}
  function openEdit(profile:ProxyConfigProfile){if(busy)return;editingId=profile.id;selectedProfile=profile;editorTitle='配置原文';discardEditor=false;showEditor=true;void editor.open(profile.id);}
  $effect(()=>{if(!mobileNavigation || !showEditor)return;return mobileNavigation.guardBeforeLeave(next=>{if(!showEditor)return true;if(saving){pendingNavigation=null;discardEditor=true;return false;}if(dirty || editorView.phase==='unknown'){pendingNavigation=next;discardEditor=true;return false;}showEditor=false;editor.close();return true;});});
  $effect(()=>{if(discardEditor && !saving && pendingNavigation===null && !dirty && editorView.phase!=='unknown')discardEditor=false;});
  function discardEditorDraft(){if(saving)return;discardEditor=false;showEditor=false;editor.close();const next=pendingNavigation;pendingNavigation=null;next?.();}
  function closeEditor(){if(saving){discardEditor=true;return;}if(dirty || editorView.phase==='unknown'){pendingNavigation=null;discardEditor=true;return;}showEditor=false;editor.close();}
  async function chooseSourceFile(){await editor.readText(async()=>{
    if(nativeHost){const receipt=await pickNativeDocument('configuration');if(receipt.status==='cancelled')return null;if(receipt.status!=='read' || receipt.content===null)throw {code:receipt.status==='failed'?'document_rejected':'document_unconfirmed'};return receipt.content;}
    const selected=await openFile({title:'选择代理配置文件',multiple:false,directory:false});if(typeof selected!=='string')return null;return readProfileEditorFile(selected);
  });}
  async function handleSave(){if(!canSave)return;if(await editor.save()){localNotice='已保存，待应用；当前选择和内核运行确认分别核对。';await refresh();}}
  function openWorkspace(){if(saving)return;if(showEditor && dirty){pendingNavigation=openWorkspace;discardEditor=true;return;}showEditor=false;editor.close();if(mobileNavigation)mobileNavigation.open('workspace');else{store.settingsSection='config';store.activeTab='settings';}}
  function openSource(){if(saving)return;showEditor=false;editor.close();const page=selectedProfile?.managedSource?'plugins':'subscriptions';if(mobileNavigation)mobileNavigation.open(page);else store.activeTab=page;}
  function requestRemove(profile:ProxyConfigProfile){if(!canRemove || busy)return;deleteTarget=profile;deleteOpen=true;}
  async function profileRemoved(){if(deleteTarget?.id===editingId){showEditor=false;editor.close();}deleteTarget=null;deleteOpen=false;await refresh();await guiState.refreshNodeStateAfterConfigChange();}
  async function handleSetActive(id:string){if(!canEdit || busy)return;activatingId=id;try{await setActiveProxyConfig(id);await refresh();await guiState.refreshNodeStateAfterConfigChange();}catch(error){handleAppError(error,'切换当前代理配置失败');}finally{activatingId=null;}}
  function editorDate(value:string|null){if(!value)return '未提供';const n=BigInt(value);return n<=8640000000000000n ? new Date(Number(n)).toLocaleString() : '时间超出可显示范围';}
  function formatDate(value:number){return new Intl.DateTimeFormat('zh-CN',{dateStyle:'medium',timeStyle:'short'}).format(value);}
  function managedPluginLabel(pluginId:string){if(pluginId==='org.zerodenet.connect.znet-sink')return 'Connect';const segment=pluginId.split('.').filter(Boolean).at(-1)??pluginId;return segment.charAt(0).toUpperCase()+segment.slice(1);}
  function getSourceLabel(profile:ProxyConfigProfile){if(profile.managedSource)return `由 ${managedPluginLabel(profile.managedSource.pluginId)} 托管`;return profile.path?'文件':'JSON';}
  $effect(() => { refresh(); });
</script>

<div class="profiles-root animate-fade-in">
  <div class="toolbar">
    <div class="toolbar-top">
      <div class="title-block">
        <div class="title">代理配置</div>
        <div class="subtitle">导入或粘贴 JSON，并在此启用与管理。</div>
      </div>

      <div class="toolbar-meta current-block">
        <span class="current-label">当前选择</span>
        <span class="current-name">{activeProfile?.name ?? '未设置'}</span>
        {#if canEdit}
          <Button size="sm" onclick={openCreate} disabled={loading || busy}>
            <Plus class="h-3.5 w-3.5" />
            <span>新建</span>
          </Button>
        {/if}
      </div>
    </div>

    <div class="toolbar-search">
      <span class="search-icon">
        <Search class="h-3.5 w-3.5" />
      </span>
      <Input bind:value={query} class="w-[260px] max-w-full pl-8" aria-label="搜索配置" placeholder="搜索名称、ID 或文件路径" />
    </div>
  </div>

  {#if localNotice}<p class="local-material-note" role="status">{localNotice}</p>{/if}
  <div class="list-shell">
    {#if loading}
      <div class="empty-state">加载中...</div>
    {:else if loadError}
      <div class="empty-state error-state" role="alert">
        <AlertTriangle class="h-5 w-5" />
        <div class="empty-title">代理配置加载失败</div>
        <div class="empty-desc">{loadError}</div>
        <Button variant="outline" onclick={refresh}>重试</Button>
      </div>
    {:else if filteredConfigs.length === 0}
      <div class="empty-state">
        <div class="empty-title">{query.trim() ? '没有匹配的代理配置' : '还没有代理配置'}</div>
        <div class="empty-desc">{query.trim() ? '请调整搜索关键词后重试。' : '新建后选择本地文件，或者直接粘贴 JSON。'}</div>
        {#if canEdit && !query.trim()}
          <Button onclick={openCreate} disabled={busy}>
            <Plus class="h-3.5 w-3.5" />
            <span>新建配置</span>
          </Button>
        {/if}
      </div>
    {:else}
      {#if mobile && !guiState.connection?.coreAvailable}<p class="local-material-note" role="status">当前显示本地配置。内核运行态未确认；已保存的材料待应用，不代表 VPN 已接管。</p>{/if}
      <div class="config-list">
        {#each filteredConfigs as config (config.id)}
          <div class="config-row">
            <button data-slot="surface-button"
              type="button"
              class="row-open"
              onclick={() => openEdit(config)}
              disabled={busy}
              aria-label={config.managedSource ? `插件托管代理配置 ${config.name}` : `编辑代理配置 ${config.name}`}
            >
              <div class="row-main">
              <div class="row-top">
                <div class="row-title">{config.name}</div>
                {#if config.active}
                  <Badge variant="secondary">当前选择</Badge>
                {:else}
                  <Badge variant="outline">未启用</Badge>
                {/if}
                <Badge variant="outline">{getSourceLabel(config)}</Badge>
              </div>

              <div class="row-meta">
                <span class="mono">{config.id}</span>
                <span>·</span>
                <span>{formatDate(config.updatedAtUnixMs)}</span>
                <span>·</span>
                <span class="row-path">{config.path ?? '内嵌 JSON'}</span>
                {#if config.managedSource}
                  <span>·</span>
                  <span>来源: {config.managedSource.sourceName ?? config.managedSource.providerId}</span>
                {/if}
              </div>
              </div>
            </button>

            <div class="row-actions">
              {#if nativeFilesAvailable && !config.managedSource}
                <NativeProfileExport profile={config} disabled={busy} />
              {/if}
              {#if canEdit}
                {#if config.active}
                  <Badge variant="secondary">运行待核对</Badge><Button variant="outline" size="sm" disabled={busy} onclick={openWorkspace}>前往工作区预检/应用</Button>
                {:else}
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => handleSetActive(config.id)}
                    disabled={busy}
                  >
                    <span>{activatingId === config.id ? '切换中...' : '设为当前'}</span>
                  </Button>
                {/if}
              {/if}

              {#if canRemove && !config.managedSource}
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onclick={() => requestRemove(config)}
                  disabled={busy}
                  title="删除"
                  aria-label="删除"
                >
                  <Trash2 class="h-3.5 w-3.5" />
                </Button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<DraggableModal title={editorTitle} description="正文按原文传输；校验与格式化由宿主执行。本地保存后仍需独立预检、应用。" open={showEditor} onClose={closeEditor} closeDisabled={saving} width="min(680px, 94vw)">
  <div class="profile-editor-form">
    {#if editorView.phase === 'loading'}<p role="status">正在读取配置原文…</p>{/if}
    {#if editorView.readOnlyReason}<p role="status" class="validation-row">{editorView.readOnlyReason}</p><Button variant="outline" onclick={openSource}>查看来源管理</Button>{/if}
    {#if editorView.phase !== 'loading' && editorView.phase !== 'error'}
      <div class="form-item"><label class="form-label" for="profile-editor-name">名称 <span class="required">*</span></label><div class="form-input-wrap"><Input id="profile-editor-name" aria-label="配置名称" value={editorView.name} oninput={event=>editor.update('name',event.currentTarget.value)} placeholder="例如：香港节点、办公配置" disabled={saving || !!editorView.readOnlyReason}/></div></div>
      {#if !editorView.readOnlyReason}<div class="form-item"><span class="form-label">读取文件</span><div class="form-input-wrap"><div class="file-picker-row"><Button variant="outline" onclick={chooseSourceFile} disabled={saving || editorView.readingFile || ['unknown','validating'].includes(editorView.phase) || (nativeHost && !nativeFilesAvailable)}><FolderOpen class="h-3.5 w-3.5"/>{editorView.readingFile ? '读取中…' : '选择配置文件'}</Button></div><p class="form-hint">{nativeHost ? nativeFilesReason : '选择文件后仅读取原文；不保存文件路径，也不启用配置。'}</p></div></div>{/if}
      {#if !editorView.readOnlyReason || editorView.sourceJson}
        <div class="form-item"><label class="form-label" for="profile-editor-json">JSON 内容</label><div class="form-input-wrap"><Textarea id="profile-editor-json" aria-label="配置 JSON 原文" value={editorView.sourceJson} oninput={event=>editor.update('sourceJson',event.currentTarget.value)} class="font-mono min-h-80" placeholder="粘贴代理配置 JSON..." rows={16} readonly={!!editorView.readOnlyReason} disabled={saving}/><p class="form-hint">保留大整数原文；保存仅写本地材料，不能据此确认运行兼容性或接管。</p></div></div>
      {/if}
    {/if}
    {#if editorView.message}<p role={editorView.issues.length || editorView.phase==='unknown' || editorView.phase==='error' ? 'alert' : 'status'} class="validation-row">{editorView.message}</p>{/if}
    {#if editorView.issues.length}<ul class="profile-editor-issues" aria-label="配置字段错误">{#each editorView.issues as issue}<li><code>{issue.fieldPath}</code>：{issue.message}{issue.line ? `（行 ${issue.line}${issue.column ? `，列 ${issue.column}` : ''}）` : ''}</li>{/each}</ul>{/if}
    {#if editorView.validationScope}<p class="form-hint">校验范围：{editorView.validationScope}；运行应用另行预检。</p>{/if}
    {#if editorView.phase==='error'}<Button variant="outline" onclick={()=>editor.open(editingId??undefined)}>重新读取原文</Button>{/if}
    {#if editorView.phase==='unknown'}<Button variant="outline" disabled={editorView.checkingLocal} onclick={()=>editor.checkLocal()}>{editorView.checkingLocal ? '核对中…' : '只读核对本地版本'}</Button>{/if}
    {#if editorView.localFacts}<dl class="profile-editor-facts" aria-label="本地版本核对资料"><dt>原目标</dt><dd>{editorView.localFacts.targetId}</dd><dt>名称</dt><dd>{editorView.localFacts.name || '未提供'}</dd><dt>本地版本时间</dt><dd>{editorDate(editorView.localFacts.updatedAtUnixMs)}</dd><dt>身份核对</dt><dd>{editorView.localFacts.matchesTarget ? '与原保存目标一致；不代表原操作成功或运行生效' : '与原目标不符'}</dd></dl>{/if}
    {#if editorView.phase==='saved'}{#if editorView.id && activeProfile?.id!==editorView.id}<Button variant="outline" onclick={()=>{const id=editorView.id;closeEditor();if(id)void handleSetActive(id);}}>启用此配置</Button>{:else}<Button variant="outline" onclick={openWorkspace}>前往工作区预检/应用</Button>{/if}{/if}
  </div>
  {#snippet footer()}
    <div class="profile-editor-footer">
      <Button variant="outline" onclick={closeEditor} disabled={saving}>关闭</Button>
      {#if !editorView.readOnlyReason && editorView.phase!=='error'}<Button variant="outline" disabled={!canValidate} onclick={()=>editor.validate()}>校验原文</Button><Button variant="outline" disabled={!canValidate} onclick={()=>editor.validate(true)}>校验并格式化</Button><Button onclick={handleSave} disabled={!canSave}>{saving ? '保存中…' : '仅保存本地配置'}</Button>{/if}
    </div>
  {/snippet}
</DraggableModal>
<ProfileRemovalDialog targetId={deleteTarget?.id ?? null} bind:open={deleteOpen} onremoved={profileRemoved}/>
<Dialog.Root bind:open={discardEditor}><Dialog.Content><Dialog.Header><Dialog.Title>{saving ? '配置正在保存' : editorView.phase==='unknown' ? '关闭结果未确认的编辑器？' : '放弃未保存的修改？'}</Dialog.Title><Dialog.Description>{saving ? '原保存操作仍在等待结果。不会取消、重放或在返回后自动离开。' : editorView.phase==='unknown' ? '放弃当前草稿并关闭，原本地保存结果仍未确认，不会撤回或重放。' : '配置正文与名称尚未保存。继续编辑可保留当前草稿；放弃只关闭编辑器。'}</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={()=>{discardEditor=false;pendingNavigation=null;}}>继续编辑</Button>{#if !saving}<Button variant="destructive" onclick={discardEditorDraft}>放弃修改</Button>{/if}</Dialog.Footer></Dialog.Content></Dialog.Root>

<style>
  .profile-editor-form {display:grid;gap:16px;min-width:0;}
  .profile-editor-footer {display:flex;gap:8px;flex-wrap:wrap;justify-content:flex-end;min-width:0;}
  .profile-editor-facts {font-size:12px;overflow-wrap:anywhere;display:grid;gap:4px;}
  .profile-editor-facts dd {margin:0 0 8px;}
  .profile-editor-issues {font-size:12px;overflow-wrap:anywhere;color:var(--destructive);}
  .profile-editor-form :global(button),.profile-editor-form :global(input),.profile-editor-footer :global(button) {min-height:44px;}
  .profile-editor-form :global(textarea),.profile-editor-form :global(input) {font-size:16px;max-width:100%;}
  .profile-editor-form :global(textarea) {min-width:0;width:100%;}
  @media(max-width:400px){.profile-editor-form .form-item{flex-direction:column;gap:6px;}.profile-editor-form .form-label{text-align:left;width:auto;}.profile-editor-form .form-input-wrap{width:100%;}.profile-editor-footer :global(button){flex:1 1 120px;}}
  .local-material-note { margin:8px 0; color:var(--muted-foreground); font-size:.85rem; line-height:1.5; }
  .profiles-root {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .toolbar,
  .list-shell {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
  }

  .toolbar {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .toolbar-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .title-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .subtitle,
  .form-hint {
    font-size: 11.5px;
    color: var(--muted-foreground);
    line-height: 1.5;
  }

  .toolbar-meta {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .current-block {
    padding: 0;
  }

  .current-label {
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .current-name {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--foreground);
  }

  .toolbar-search {
    position: relative;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted-foreground);
    pointer-events: none;
  }

  .list-shell {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .config-list {
    height: 100%;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .config-row {
    display: flex;
    align-items: stretch;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    transition: background 0.12s ease;
  }

  .config-row:hover {
    background: var(--muted);
  }

  .config-row:last-child {
    border-bottom: 0;
  }

  .row-open {
    all: unset;
    flex: 1;
    min-width: 0;
    display: flex;
    cursor: pointer;
    border-radius: 6px;
  }

  .row-open:focus-visible {
    box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.3);
  }

  .row-open:disabled {
    cursor: wait;
    opacity: 0.65;
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row-top {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .row-title {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--foreground);
  }

  .row-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .row-path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 420px;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    align-self: center;
  }

  .empty-state {
    min-height: 240px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 20px;
    text-align: center;
    color: var(--muted-foreground);
  }

  .empty-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .empty-desc {
    font-size: 11.5px;
    line-height: 1.5;
  }

  .error-state {
    color: var(--destructive);
  }

  /* Form styles (layout provided by DraggableModal) */

  .form-item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .form-label {
    flex-shrink: 0;
    width: 72px;
    padding-top: 7px;
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
    text-align: right;
  }

  .form-input-wrap {
    flex: 1;
    min-width: 0;
  }

  .required { color: var(--destructive); }

  .form-hint { margin-top: 4px; }

  :global(.source-switch) {
    display: inline-grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .file-picker-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .validation-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.2);
    color: var(--warning);
    font-size: 11.5px;
  }

  :global(.sr-only) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 600px) {
    .toolbar-top {
      flex-direction: column;
      align-items: stretch;
    }

    .toolbar-meta {
      justify-content: flex-start;
    }
  }

  @media (max-width: 640px) {
    .row-actions,
    .file-picker-row {
      flex-direction: column;
      align-items: stretch;
    }

    :global(.source-switch) {
      grid-template-columns: 1fr;
    }

    .config-row {
      flex-direction: column;
    }

    .row-actions {
      align-self: stretch;
    }

    .row-path {
      max-width: none;
    }
  }
</style>
