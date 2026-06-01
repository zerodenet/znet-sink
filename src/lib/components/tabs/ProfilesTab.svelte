<script lang="ts">
  import { open as openFile } from '@tauri-apps/plugin-dialog';
  import { store } from '$lib/services/store.svelte';
  import { handleAppError } from '$lib/services/core';
  import {
    importProxyConfig,
    listProxyConfigs,
    removeProxyConfig,
    setActiveProxyConfig,
    upsertProxyConfig,
  } from '$lib/services/config';
  import type { ProxyConfigProfile, ProxyConfigUpsert } from '$lib/types/domain';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import { AlertTriangle, FileJson, FolderOpen, Plus, Search, Trash2, X } from '@lucide/svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';

  type SourceMode = 'file' | 'inline';

  type EditorDraft = {
    name: string;
    sourceMode: SourceMode;
    sourcePath: string;
    content: string;
    active: boolean;
  };

  let configs = $state<ProxyConfigProfile[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let activatingId = $state<string | null>(null);
  let query = $state('');
  let showEditor = $state(false);
  let editingId = $state<string | null>(null);
  let editorTitle = $state('新建代理配置');
  let draft = $state<EditorDraft>(emptyDraft());
  let validationMessage = $state<string | null>(null);
  let selectedProfile = $state<ProxyConfigProfile | null>(null);

  const canEdit = $derived(store.isActionOperable('proxyConfig.upsert'));
  const canRemove = $derived(store.isActionOperable('proxyConfig.remove'));
  const filteredConfigs = $derived(filterConfigs(configs, query));
  const activeProfile = $derived(configs.find((item) => item.active) ?? null);
  const canSave = $derived(canEdit && !saving && draft.name.trim().length > 0 && isDraftReady(draft));

  function emptyDraft(mode: SourceMode = 'file'): EditorDraft {
    return {
      name: '',
      sourceMode: mode,
      sourcePath: '',
      content: '{}',
      active: false,
    };
  }

  function filterConfigs(items: ProxyConfigProfile[], value: string): ProxyConfigProfile[] {
    const term = value.trim().toLowerCase();
    if (!term) return items;

    return items.filter((item) => {
      return [item.name, item.id, item.path ?? '']
        .join(' ')
        .toLowerCase()
        .includes(term);
    });
  }

  function stringifyJson(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return '{}';
    }
  }

  function parseJson(text: string): { ok: true; value: unknown } | { ok: false; error: string } {
    const trimmed = text.trim();
    if (!trimmed) {
      return { ok: false, error: '请粘贴代理配置 JSON' };
    }

    try {
      return { ok: true, value: JSON.parse(trimmed) };
    } catch (error) {
      return {
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  function isDraftReady(state: EditorDraft): boolean {
    if (state.sourceMode === 'file') {
      return Boolean(state.sourcePath.trim());
    }

    return parseJson(state.content).ok;
  }

  async function refresh() {
    loading = true;
    try {
      configs = await listProxyConfigs();
    } catch (error) {
      handleAppError(error, '加载代理配置失败');
    } finally {
      loading = false;
    }
  }

  function openCreate() {
    draft = emptyDraft('file');
    editingId = null;
    selectedProfile = null;
    validationMessage = null;
    editorTitle = '新建代理配置';
    showEditor = true;
  }

  function openEdit(profile: ProxyConfigProfile) {
    draft = {
      name: profile.name,
      sourceMode: profile.path ? 'file' : 'inline',
      sourcePath: profile.path ?? '',
      content: profile.content !== undefined && profile.content !== null ? stringifyJson(profile.content) : '{}',
      active: profile.active,
    };
    editingId = profile.id;
    selectedProfile = profile;
    validationMessage = null;
    editorTitle = '编辑代理配置';
    showEditor = true;
  }

  function closeEditor() {
    if (saving) return;
    showEditor = false;
    validationMessage = null;
  }

  function setSourceMode(mode: SourceMode) {
    if (draft.sourceMode === mode) return;
    draft.sourceMode = mode;
    validationMessage = null;

    if (mode === 'inline' && !draft.content.trim()) {
      draft.content = selectedProfile?.content !== undefined && selectedProfile?.content !== null
        ? stringifyJson(selectedProfile.content)
        : '{}';
    }
  }

  async function chooseSourceFile() {
    const selected = await openFile({
      title: '选择代理配置文件',
      multiple: false,
      directory: false,
      defaultPath: draft.sourcePath || selectedProfile?.path || undefined,
    });

    if (typeof selected === 'string' && selected.trim()) {
      draft.sourcePath = selected.trim();
      validationMessage = null;
    }
  }

  async function handleSave() {
    if (!canSave) return;

    validationMessage = null;
    saving = true;

    try {
      if (draft.sourceMode === 'file') {
        if (!draft.sourcePath.trim()) {
          validationMessage = '请选择代理配置文件';
          return;
        }

        await importProxyConfig({
          id: editingId ?? undefined,
          name: draft.name.trim(),
          path: draft.sourcePath.trim(),
          active: draft.active,
        });
      } else {
        const parsed = parseJson(draft.content);
        if (!parsed.ok) {
          validationMessage = parsed.error;
          return;
        }

        const input: ProxyConfigUpsert = {
          id: editingId ?? undefined,
          name: draft.name.trim(),
          content: parsed.value,
          active: draft.active,
        };
        await upsertProxyConfig(input);
      }

      await refresh();
      closeEditor();
    } catch (error) {
      handleAppError(error, '保存代理配置失败');
    } finally {
      saving = false;
    }
  }

  async function handleRemove(id: string) {
    if (!canRemove) return;
    if (!confirm('确认删除此代理配置？')) return;

    try {
      await removeProxyConfig(id);
      if (editingId === id) {
        closeEditor();
      }
      await refresh();
    } catch (error) {
      handleAppError(error, '删除代理配置失败');
    }
  }

  async function handleSetActive(id: string) {
    if (!canEdit) return;

    activatingId = id;
    try {
      await setActiveProxyConfig(id);
      await refresh();
    } catch (error) {
      handleAppError(error, '切换当前代理配置失败');
    } finally {
      activatingId = null;
    }
  }

  function formatDate(value: number): string {
    return new Intl.DateTimeFormat('zh-CN', {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(value);
  }

  function getSourceLabel(profile: ProxyConfigProfile): string {
    return profile.path ? '文件' : 'JSON';
  }

  function getSaveLabel(): string {
    return draft.sourceMode === 'file' ? '导入并保存' : '保存配置';
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="profiles-root animate-fade-in">
  <div class="toolbar">
    <div class="toolbar-top">
      <div class="title-block">
        <div class="title">代理配置</div>
        <div class="subtitle">只保留静态配置导入和 JSON 粘贴。编辑、启用、删除都在这里完成。</div>
      </div>

      <div class="toolbar-meta current-block">
        <span class="current-label">当前配置</span>
        <span class="current-name">{activeProfile?.name ?? '未设置'}</span>
        {#if canEdit}
          <Button size="sm" onclick={openCreate} disabled={loading}>
            <Plus class="h-3.5 w-3.5" />
            <span>新建配置</span>
          </Button>
        {/if}
      </div>
    </div>

    <div class="toolbar-search">
      <span class="search-icon">
        <Search class="h-3.5 w-3.5" />
      </span>
      <input bind:value={query} class="search-input" placeholder="搜索名称、ID 或文件路径" />
    </div>
  </div>

  <div class="list-shell">
    {#if loading}
      <div class="empty-state">加载中...</div>
    {:else if filteredConfigs.length === 0}
      <div class="empty-state">
        <div class="empty-title">还没有代理配置</div>
        <div class="empty-desc">新建后选择本地文件，或者直接粘贴 JSON。</div>
        {#if canEdit}
          <Button onclick={openCreate}>
            <Plus class="h-3.5 w-3.5" />
            <span>新建配置</span>
          </Button>
        {/if}
      </div>
    {:else}
      <div class="config-list">
        {#each filteredConfigs as config (config.id)}
          <div
            class="config-row"
            role="button"
            tabindex="0"
            onclick={() => openEdit(config)}
            onkeydown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                openEdit(config);
              }
            }}
          >
            <div class="row-main">
              <div class="row-top">
                <div class="row-title">{config.name}</div>
                {#if config.active}
                  <Badge variant="secondary">当前生效</Badge>
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
              </div>
            </div>

            <div class="row-actions">
              {#if canEdit}
                {#if config.active}
                  <Badge variant="secondary">已启用</Badge>
                {:else}
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={(event) => {
                      event.stopPropagation();
                      handleSetActive(config.id);
                    }}
                    disabled={activatingId === config.id}
                  >
                    <span>{activatingId === config.id ? '切换中...' : '设为当前'}</span>
                  </Button>
                {/if}
              {/if}

              {#if canRemove}
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onclick={(event) => {
                    event.stopPropagation();
                    handleRemove(config.id);
                  }}
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

<DraggableModal
  title={editorTitle}
  description="仅支持固定 JSON 配置。选择文件或直接粘贴内容，解析失败会直接报错。"
  open={showEditor}
  onClose={closeEditor}
  closeDisabled={saving}
  width="min(680px, 90vw)"
>
    <div class="form-item">
      <span class="form-label">名称 <span class="required">*</span></span>
      <div class="form-input-wrap">
        <Input bind:value={draft.name} placeholder="例如：香港节点、办公配置" disabled={saving} />
      </div>
    </div>

    <div class="form-item">
      <span class="form-label">导入方式</span>
      <div class="form-input-wrap">
      <div class="source-switch">
        <button
          type="button"
          class="source-btn"
          class:active={draft.sourceMode === 'file'}
          onclick={() => setSourceMode('file')}
          disabled={saving}
        >
          <FolderOpen class="h-3.5 w-3.5" />
          <span>本地文件</span>
        </button>
        <button
          type="button"
          class="source-btn"
          class:active={draft.sourceMode === 'inline'}
          onclick={() => setSourceMode('inline')}
          disabled={saving}
        >
          <FileJson class="h-3.5 w-3.5" />
          <span>粘贴 JSON</span>
        </button>
      </div>
      </div>
    </div>

    {#if draft.sourceMode === 'file'}
      <div class="form-item">
        <span class="form-label">配置文件</span>
        <div class="form-input-wrap">
        <div class="file-picker-row">
          <Input value={draft.sourcePath} readonly placeholder="请选择本地配置文件" class="mono" />
          <Button variant="outline" size="sm" onclick={chooseSourceFile} disabled={saving}>
            <FolderOpen class="h-3.5 w-3.5" />
            <span>选择</span>
          </Button>
        </div>
        <div class="form-hint">保存时会直接读取这个文件并解析为 JSON。</div>
        </div>
      </div>
    {:else}
      <div class="form-item">
        <span class="form-label">JSON 内容</span>
        <div class="form-input-wrap">
          <textarea
            bind:value={draft.content}
            class="json-editor mono"
            placeholder="粘贴代理配置 JSON..."
            rows={16}
            disabled={saving}
          ></textarea>
          <div class="form-hint">不做格式选择，不做内核选择，保存前只检查 JSON 是否可解析。</div>
        </div>
      </div>
    {/if}

    <div class="form-item">
      <span class="form-label">设为当前</span>
      <div class="form-input-wrap flex items-center">
        <span class="switch-copy">{draft.active ? '保存后立即切换到这份配置' : '仅保存，不切换当前配置'}</span>
        <Switch bind:checked={draft.active} disabled={saving} />
      </div>
    </div>

    {#if validationMessage}
      <div class="validation-row">
        <AlertTriangle class="h-3.5 w-3.5" />
        <span>{validationMessage}</span>
      </div>
    {/if}

  {#snippet footer()}
    <Button variant="outline" onclick={closeEditor} disabled={saving}>取消</Button>
    <Button onclick={handleSave} disabled={!canSave}>
      <span>{saving ? '保存中...' : getSaveLabel()}</span>
    </Button>
  {/snippet}
</DraggableModal>

<style>
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
  .form-hint,
  .switch-copy {
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

  .search-input {
    width: 100%;
    height: 36px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    padding: 0 12px 0 30px;
    outline: none;
    transition: border-color 0.15s ease, background 0.15s ease;
  }

  .search-input:focus {
    border-color: rgba(99, 102, 241, 0.24);
    background: var(--background);
  }

  .search-input::placeholder {
    color: var(--muted-foreground);
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
    cursor: pointer;
    transition: background 0.12s ease;
  }

  .config-row:hover {
    background: var(--muted);
  }

  .config-row:last-child {
    border-bottom: 0;
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

  .source-switch {
    display: inline-grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .source-btn {
    height: 36px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .source-btn.active {
    background: rgba(99, 102, 241, 0.08);
    border-color: rgba(99, 102, 241, 0.24);
    color: var(--foreground);
  }

  .file-picker-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .json-editor {
    width: 100%;
    min-height: 320px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    line-height: 1.6;
    resize: vertical;
    outline: none;
  }

  .json-editor:focus {
    border-color: rgba(99, 102, 241, 0.24);
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

  @media (max-width: 900px) {
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

    .source-switch {
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
