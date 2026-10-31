<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import { endpointForms } from './endpoint-forms';
  import { Button } from '$lib/components/ui/button';
  import { getAppErrorMessage } from '$lib/services/core';
  import { loadUserEndpoints, saveUserEndpoints, type UserEndpoint, type UserEndpointLibrary } from '$lib/features/endpoints/library';
  let { onSaved, onChange, disabled = false, busy = $bindable(false) }: { onSaved: () => void; onChange: (rows: UserEndpoint[]) => void; disabled?: boolean; busy?: boolean } = $props();
  let library = $state<UserEndpointLibrary | null>(null);
  let error = $state('');
  let notice = $state('');
  let draft = $state<UserEndpoint | null>(null);
  let originalTag = $state<string | null>(null);
  let deleting = $state<string | null>(null);
  let editorOpen = $state(false);
  let deleteOpen = $state(false);
  const protocolOptions = $derived((library?.protocols ?? []).filter(protocol => endpointForms[protocol.type]).map(protocol => ({ value: protocol.type, label: protocol.label })));
  const formAdapter = $derived(draft ? endpointForms[draft.protocol.type] : undefined);
  function changeProtocol(protocol: string) {
    if (!draft || originalTag !== null || !endpointForms[protocol]) return;
    const next = endpointForms[protocol].create();
    next.tag = draft.tag; next.enabled = draft.enabled; next.directions = draft.directions;
    draft = next;
  }
  let alive = true;
  function closeEditor(open: boolean) { if (!busy) { editorOpen = open; if (!open) { draft = null; error = ''; } } }
  function closeDelete(open: boolean) { if (!busy) { deleteOpen = open; if (!open) { deleting = null; error = ''; } } }
  export async function refresh() {
    busy = true;
    error = '';
    try { const next = await loadUserEndpoints(); if (alive) { library = next; onChange(next.endpoints); } }
    catch (e) { if (alive) error = getAppErrorMessage(e, '用户端点读取失败'); }
    finally { if (alive) busy = false; }
  }
  onMount(() => { alive = true; void refresh(); return () => { alive = false; draft = null; library = null; }; });
  export async function edit(tag: string | null = null) {
    if (busy || disabled) return;
    originalTag = tag; editorOpen = true; draft = null; deleting = null; notice = '';
    await refresh();
    if (!library || error || !alive) return;
    const endpoint = tag === null ? endpointForms[protocolOptions[0]?.value]?.create() : library.endpoints.find(row => row.tag === tag);
    if (!endpoint) { error = tag === null ? '当前客户端没有可用的端点编辑器' : '端点已被删除，请刷新'; return; }
    if (!endpointForms[endpoint.protocol.type]) { error = '当前客户端尚无此协议的编辑器，请升级客户端'; return; }
    originalTag = tag;
    draft = structuredClone($state.snapshot(endpoint));
    deleting = null; notice = '';
  }
  export async function remove(tag: string) {
    if (busy || disabled) return;
    await refresh();
    if (!library || error || !alive) return;
    if (!library.endpoints.some(row => row.tag === tag)) { error = '端点已被删除，请刷新'; return; }
    deleting = tag; draft = null; editorOpen = false; deleteOpen = true; notice = '';
  }
  async function commit(remove = false) {
    if (!library || busy || disabled) return;
    let endpoints = library.endpoints;
    if (remove) endpoints = endpoints.filter(row => row.tag !== deleting);
    else {
      if (!draft) return;
      const row = structuredClone($state.snapshot(draft));
      row.tag = row.tag.trim();
      if (endpoints.some(endpoint => endpoint.tag === row.tag && endpoint.tag !== originalTag)) { error = '用户端点名称不能重复'; return; }
      endpoints = originalTag === null ? [...endpoints, row] : endpoints.map(endpoint => endpoint.tag === originalTag ? row : endpoint);
    }
    busy = true; error = ''; notice = '';
    try {
      const next = await saveUserEndpoints(library.revision, endpoints);
      if (!alive) return;
      library = next; onChange(next.endpoints); draft = null; deleting = null; editorOpen = false; deleteOpen = false;
      notice = '已保存；内核运行时已应用，未运行时将在下次启动生效。'; onSaved();
    } catch (e) { if (alive) error = getAppErrorMessage(e, '用户端点保存失败，请刷新后核对实际状态'); }
    finally { if (alive) busy = false; }
  }
</script>
{#if !editorOpen && !deleteOpen}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if notice}<p role="status" class="notice">{notice}</p>{/if}
{/if}
<Dialog.Root open={editorOpen} onOpenChange={closeEditor}>
  <Dialog.Content class="sm:max-w-[560px]" showCloseButton={!busy} onEscapeKeydown={event => { if (busy) event.preventDefault(); }} onInteractOutside={event => { if (busy) event.preventDefault(); }}>
    <Dialog.Header>
      <Dialog.Title>{originalTag === null ? '创建端点' : '编辑端点'}</Dialog.Title>
      <Dialog.Description>自动加入所有配置，切换配置后仍保留。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body>
      {#if error}<p role="alert" class="error">{error}</p>{/if}
      {#if draft}
        <form id="endpoint-editor-form" onsubmit={event => { event.preventDefault(); void commit(); }}>
          <fieldset disabled={busy || disabled} class="grid min-w-0 gap-4 border-0 p-0">
            <label class="grid gap-1.5 text-xs"><span>端点名称 <span class="text-destructive" aria-hidden="true">*</span></span><Input required maxlength={128} bind:value={draft.tag} disabled={busy || disabled} autocomplete="off" placeholder="例如：家庭网络" /></label>
            <div class="grid gap-1.5 text-xs"><span id="endpoint-protocol-label">协议</span><FieldSelect value={draft.protocol.type} onValueChange={changeProtocol} options={protocolOptions} disabled={busy || disabled || originalTag !== null} aria-labelledby="endpoint-protocol-label" /></div>
            <div class="flex flex-wrap items-center gap-5 text-xs">
              <div class="flex items-center gap-2"><span id="endpoint-enabled-label">启用</span><Switch checked={draft.enabled ?? true} onCheckedChange={value => { if (draft) draft.enabled = value; }} disabled={busy || disabled} aria-labelledby="endpoint-enabled-label" /></div>
              {#if draft.directions}
                <div class="flex items-center gap-2"><span id="endpoint-inbound-label">入站</span><Switch bind:checked={draft.directions.inbound} disabled={busy || disabled} aria-labelledby="endpoint-inbound-label" /></div>
                <div class="flex items-center gap-2"><span id="endpoint-outbound-label">出站</span><Switch bind:checked={draft.directions.outbound} disabled={busy || disabled} aria-labelledby="endpoint-outbound-label" /></div>
              {/if}
            </div>
            {#if formAdapter}
              {@const Form = formAdapter.component}
              {#key draft}<Form bind:endpoint={draft} disabled={busy || disabled} />{/key}
            {/if}
          </fieldset>
        </form>
      {:else if busy}<p role="status">正在读取端点…</p>{/if}
    </Dialog.Body>
    <Dialog.Footer>
      <Button type="button" variant="outline" disabled={busy} onclick={() => closeEditor(false)}>取消</Button>
      <Button type="submit" form="endpoint-editor-form" disabled={busy || disabled || !draft}>{busy ? '正在保存…' : '保存并应用'}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<Dialog.Root open={deleteOpen} onOpenChange={closeDelete}>
  <Dialog.Content class="sm:max-w-[440px]" showCloseButton={!busy} onEscapeKeydown={event => { if (busy) event.preventDefault(); }} onInteractOutside={event => { if (busy) event.preventDefault(); }}>
    <Dialog.Header><Dialog.Title>删除端点 {deleting}？</Dialog.Title><Dialog.Description>此端点将从所有配置中移除，相关连接可能中断。</Dialog.Description></Dialog.Header>
    <Dialog.Body><p>请先调整引用它的策略、路由或 DNS。</p>{#if error}<p role="alert" class="error">{error}</p>{/if}</Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={busy} onclick={() => closeDelete(false)}>取消</Button><Button variant="destructive" disabled={busy || disabled} onclick={() => commit(true)}>{busy ? '正在删除…' : '确认删除'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  .error { color:var(--destructive); font-size:12px; margin:0 0 12px; }
  .notice { color:var(--muted-foreground); font-size:12px; margin:10px 0; }
</style>
