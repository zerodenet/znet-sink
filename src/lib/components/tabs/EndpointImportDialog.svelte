<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Switch } from '$lib/components/ui/switch';
  import { chooseImportedDns, mergeImportedEndpoints } from '$lib/features/endpoints/dns';
  import { parseWireGuardConfig } from '$lib/features/endpoints/protocols/wireguard-import';
  import { validateWireGuardEndpoint } from '$lib/features/endpoints/protocols/wireguard';
  import WireGuardForm from './endpoint-forms/WireGuardForm.svelte';
  import { Input } from '$lib/components/ui/input';
  import { getAppErrorMessage } from '$lib/services/core';
  import { saveUserEndpoints, type UserEndpointLibrary } from '$lib/features/endpoints/library';
  import { readWireGuardFile, type WireGuardFilePreview } from '$lib/features/endpoints/protocols/wireguard-file';
  let { busy = $bindable(false), library, onSaved, onCancel, mode = 'file' }: {
    busy?: boolean; library: UserEndpointLibrary; onSaved: (next: UserEndpointLibrary) => void; onCancel: () => void; mode?: 'file' | 'paste';
  } = $props();
  let text = $state('');
  let name = $state('');
  let rows = $state<WireGuardFilePreview[]>([]);
  let error = $state('');
  let picker = $state<HTMLInputElement | null>(null);
  let alive = true;
  onMount(() => () => { alive = false; rows = []; });
  const problems = $derived.by(() => {
    const tags = new Set(library.endpoints.map(row => row.tag));
    return rows.map(row => {
      if (row.error || !row.endpoint) return row.error;
      const tag = row.endpoint.tag.trim();
      if (!tag || new TextEncoder().encode(tag).length > 128) return '端点名称须为 1～128 字节。';
      if (tags.has(tag)) return '名称与已有端点或本次导入重复，请修改名称。';
      tags.add(tag); return validateWireGuardEndpoint(row.endpoint)[0]?.message ?? '';
    });
  });
  const canSave = $derived(rows.length > 0 && !problems.some(Boolean) && rows.every(row => row.endpoint));
  function prepare(next: WireGuardFilePreview[]) {
    const endpoints = chooseImportedDns(next.flatMap(row => row.endpoint ? [row.endpoint] : []));
    let index = 0;
    rows = next.map(row => row.endpoint ? { ...row, endpoint: endpoints[index++] } : row);
  }
  function chooseDefault(index: number, enabled: boolean) {
    rows.forEach((row, i) => { if (row.endpoint?.dns && (enabled || i === index)) row.endpoint.dns.use_as_default = enabled && i === index; });
  }
  function parseText() {
    if (busy) return;
    error = ''; rows = [];
    if (new TextEncoder().encode(text).length > 256 * 1024) { error = '粘贴配置不能超过 256 KiB。'; return; }
    try {
      const result = parseWireGuardConfig(text, (name.trim() || '粘贴配置') + '.conf');
      prepare([{ filename: '粘贴配置', ...result, error: '' }]);
    } catch (cause) { error = cause instanceof Error ? cause.message : '配置解析失败'; }
  }
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
      if (alive) prepare(next);
    } finally { input.value = ''; if (alive) busy = false; }
  }
  async function save() {
    if (!canSave || busy) return;
    busy = true; error = '';
    try {
      const endpoints = rows.map(row => ({ ...$state.snapshot(row.endpoint!), tag: row.endpoint!.tag.trim() }));
      const next = await saveUserEndpoints(library.revision, mergeImportedEndpoints($state.snapshot(library.endpoints), endpoints));
      if (!alive) return;
      onSaved(next); rows = []; text = '';
    } catch (e) { if (alive) error = getAppErrorMessage(e, '导入保存失败，请刷新后核对实际状态'); }
    finally { if (alive) busy = false; }
  }
</script>
<Dialog.Body class="grid gap-4">
  {#if mode === 'file'}
    <Input bind:ref={picker} type="file" accept=".conf,.ini" multiple class="sr-only" aria-label="选择 WireGuard 配置文件" disabled={busy} onchange={select} />
    <Button variant="outline" disabled={busy} onclick={() => picker?.click()}>选择配置文件</Button>
    <p class="text-xs text-muted-foreground">支持选择多个文件，每个文件创建一个端点。解析后可展开参数检查和修改，确认前不会保存。</p>
  {:else}
    <label class="grid gap-1.5 text-xs"><span>端点名称</span><Input bind:value={name} disabled={busy} placeholder="例如：家庭网络" /></label>
    <label class="grid gap-1.5 text-xs"><span>WireGuard 配置文本</span><Textarea class="min-h-32 font-mono" aria-label="WireGuard 配置文本" bind:value={text} disabled={busy} spellcheck="false" autocomplete="off" maxlength={262144} placeholder="[Interface]&#10;PrivateKey = ...&#10;Address = ...&#10;DNS = ...&#10;&#10;[Peer]&#10;..." /></label>
    <Button variant="outline" disabled={busy || !text.trim()} onclick={parseText}>解析并预览</Button>
  {/if}
  <p class="text-xs text-muted-foreground">DNS 与端点一起保存和应用。选择一个默认 DNS 来源，原有 DNS 规则继续保留。</p>
  {#if error}<p role="alert" class="text-xs text-destructive">{error}</p>{/if}
  {#each rows as row, index}
    <article class="grid min-w-0 gap-3 rounded-lg border border-border p-4" aria-label={`导入文件 ${row.filename}`}>
      <p class="break-all text-sm font-medium">{row.filename}</p>
      {#if row.endpoint}
        <label class="grid gap-1.5 text-xs"><span>端点名称</span><Input bind:value={row.endpoint.tag} disabled={busy} aria-invalid={!!problems[index]} /></label>
        <p class="break-words text-xs text-muted-foreground">WireGuard · {(row.endpoint.protocol.addresses as string[]).join(', ')} · {(row.endpoint.protocol.peers as unknown[]).length} 个 Peer</p>
        {#if row.endpoint.dns}
          <p class="text-xs text-muted-foreground">DNS：{row.endpoint.dns.servers.join(', ')}</p>
          <div class="flex items-center gap-2 text-xs"><span id={`import-dns-${index}`}>用作默认 DNS：{row.endpoint.tag}</span><Switch aria-labelledby={`import-dns-${index}`} checked={row.endpoint.dns.use_as_default} onCheckedChange={value => chooseDefault(index, value)} disabled={busy}/></div>
        {/if}
        <details><summary class="cursor-pointer text-xs">查看和修改连接参数</summary><div class="mt-4">{#key row.endpoint}<WireGuardForm bind:endpoint={row.endpoint} disabled={busy} dnsDefaultChoice={false}/>{/key}</div></details>
      {/if}
      {#if problems[index]}<p role="alert" class="text-xs text-destructive">{problems[index]}</p>{/if}
      {#each row.warnings as warning}<p role="note" class="text-xs text-muted-foreground">{warning}</p>{/each}
    </article>
  {/each}
</Dialog.Body>
<Dialog.Footer>
  <Button variant="outline" disabled={busy} onclick={onCancel}>取消</Button>
  <Button disabled={busy || !canSave} onclick={save}>{busy ? '正在处理…' : '确认导入并应用'}</Button>
</Dialog.Footer>
