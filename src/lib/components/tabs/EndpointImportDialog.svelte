<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getAppErrorMessage } from '$lib/services/core';
  import { saveUserEndpoints, type UserEndpointLibrary } from '$lib/features/endpoints/library';
  import { readWireGuardFile, type WireGuardFilePreview } from '$lib/features/endpoints/protocols/wireguard-file';
  let { open = $bindable(false), busy = $bindable(false), library, onSaved }: {
    open?: boolean; busy?: boolean; library: UserEndpointLibrary; onSaved: (next: UserEndpointLibrary) => void;
  } = $props();
  let rows = $state<WireGuardFilePreview[]>([]);
  let error = $state('');
  let picker: HTMLInputElement;
  let alive = true;
  onMount(() => () => { alive = false; rows = []; });
  const problems = $derived.by(() => {
    const tags = new Set(library.endpoints.map(row => row.tag));
    return rows.map(row => {
      if (row.error || !row.endpoint) return row.error;
      const tag = row.endpoint.tag.trim();
      if (!tag || new TextEncoder().encode(tag).length > 128) return '端点名称须为 1～128 字节。';
      if (tags.has(tag)) return '名称与已有端点或本次导入重复，请修改名称。';
      tags.add(tag); return '';
    });
  });
  const canSave = $derived(rows.length > 0 && !problems.some(Boolean) && rows.every(row => row.endpoint));
  function close(next: boolean) { if (!busy) open = next; }
  async function select(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    if (!files.length || busy) return;
    error = '';
    if (files.length + library.endpoints.length > 256 || files.reduce((sum, file) => sum + file.size, 0) > 4 * 1024 * 1024) {
      rows = []; error = '用户端点总数不能超过 256 个；本次文件总大小不能超过 4 MiB。'; input.value = ''; return;
    }
    busy = true;
    try {
      const next = await Promise.all(files.map(readWireGuardFile));
      if (alive) rows = next;
    } finally { input.value = ''; if (alive) busy = false; }
  }
  async function save() {
    if (!canSave || busy) return;
    busy = true; error = '';
    try {
      const endpoints = rows.map(row => ({ ...$state.snapshot(row.endpoint!), tag: row.endpoint!.tag.trim() }));
      const next = await saveUserEndpoints(library.revision, [...library.endpoints, ...endpoints]);
      if (!alive) return;
      onSaved(next); rows = []; open = false;
    } catch (e) { if (alive) error = getAppErrorMessage(e, '导入保存失败，请刷新后核对实际状态'); }
    finally { if (alive) busy = false; }
  }
</script>
<Dialog.Root {open} onOpenChange={close}>
  <Dialog.Content class="sm:max-w-[600px]" showCloseButton={!busy} onEscapeKeydown={event => { if (busy) event.preventDefault(); }} onInteractOutside={event => { if (busy) event.preventDefault(); }}>
    <Dialog.Header>
      <Dialog.Title>导入 WireGuard 配置</Dialog.Title>
      <Dialog.Description>支持选择多个 .conf 文件，每个文件创建一个端点，保留所有 Peer。确认后自动加入所有配置。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="grid gap-4">
      <input bind:this={picker} type="file" accept=".conf,.ini" multiple class="sr-only" aria-label="选择 WireGuard 配置文件" disabled={busy} onchange={select} />
      <Button variant="outline" disabled={busy} onclick={() => picker.click()}>选择配置文件</Button>
      <p class="text-xs text-muted-foreground">文件仅在本机读取。先预览并确认，再保存和应用；不会覆盖已有端点。</p>
      {#if error}<p role="alert" class="text-xs text-destructive">{error}</p>{/if}
      {#each rows as row, index}
        <article class="grid min-w-0 gap-3 rounded-lg border border-border p-4" aria-label={`导入文件 ${row.filename}`}>
          <p class="break-all text-sm font-medium">{row.filename}</p>
          {#if row.endpoint}
            <label class="grid gap-1.5 text-xs"><span>端点名称</span><Input bind:value={row.endpoint.tag} disabled={busy} aria-invalid={!!problems[index]} /></label>
            <p class="break-words text-xs text-muted-foreground">WireGuard · {(row.endpoint.protocol.addresses as string[]).join(', ')} · {(row.endpoint.protocol.peers as unknown[]).length} 个 Peer</p>
          {/if}
          {#if problems[index]}<p role="alert" class="text-xs text-destructive">{problems[index]}</p>{/if}
          {#each row.warnings as warning}<p role="note" class="text-xs text-muted-foreground">{warning}</p>{/each}
        </article>
      {/each}
    </Dialog.Body>
    <Dialog.Footer>
      <Button variant="outline" disabled={busy} onclick={() => close(false)}>取消</Button>
      <Button disabled={busy || !canSave} onclick={save}>{busy ? '正在处理…' : '确认导入并应用'}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
