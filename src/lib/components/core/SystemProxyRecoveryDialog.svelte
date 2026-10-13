<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { ShieldCheck, RotateCcw, LoaderCircle } from '@lucide/svelte';
  import { guiState } from '$lib/services/gui-state.svelte';

  const open = $derived(guiState.systemProxyRecoveryOpen);
  let pending = $state<'takeover' | 'reset' | null>(null);
  let error = $state<string | null>(null);
  const locked = $derived(pending !== null || !guiState.canResetSystemProxy);
  const current = $derived(guiState.connectionError || !guiState.connection ? '系统代理状态待确认'
    : !guiState.isSystemProxyEnabled ? '系统代理已关闭'
    : guiState.connection.systemProxyHost && guiState.connection.systemProxyPort
      ? `${guiState.connection.systemProxyHost}:${guiState.connection.systemProxyPort}` : '系统代理已开启');
  $effect(() => { if (!open && !pending) error = null; });
  const target = $derived(guiState.connection?.localProxyHost && guiState.connection?.localProxyPort
    ? `${guiState.connection.localProxyHost}:${guiState.connection.localProxyPort}` : '客户端的代理端口');

  async function recover(action: 'takeover' | 'reset') {
    if (locked || (action === 'takeover' && !guiState.canEnableSystemProxy)) return;
    pending = action;
    error = null;
    try {
      const result = await (action === 'takeover' ? guiState.enableSystemProxy() : guiState.resetSystemProxy());
      if (result.ok) guiState.systemProxyRecoveryOpen = false;
      else error = result.message;
    } finally { pending = null; }
  }
</script>

<Dialog.Root bind:open={guiState.systemProxyRecoveryOpen} onOpenChange={(value) => { if (pending) guiState.systemProxyRecoveryOpen = true; else if (value) error = null; }}>
  <Dialog.Content class="sm:max-w-md" showCloseButton={!pending}
    onEscapeKeydown={(event) => { if (pending) event.preventDefault(); }}
    onInteractOutside={(event) => { if (pending) event.preventDefault(); }}>
    <Dialog.Header>
      <Dialog.Title>处理系统代理</Dialog.Title>
      <Dialog.Description>选择接管或清除当前系统代理。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="space-y-3">
      <div class="rounded-lg bg-muted px-3 py-2 font-mono text-sm break-all">{current}</div>
      <Button variant="outline" class="h-auto w-full justify-start gap-3 py-3 text-left"
        disabled={locked || !guiState.canEnableSystemProxy} onclick={() => recover('takeover')}>
        {#if pending === 'takeover'}<LoaderCircle class="h-5 w-5 animate-spin" />{:else}<ShieldCheck class="h-5 w-5 text-primary" />{/if}
        <span class="min-w-0"><span class="block font-semibold">强制接管</span>
          <span class="block whitespace-normal text-xs text-muted-foreground">替换为 {target}，由本应用管理</span></span>
      </Button>
      <Button variant="outline" class="h-auto w-full justify-start gap-3 py-3 text-left"
        disabled={locked} onclick={() => recover('reset')}>
        {#if pending === 'reset'}<LoaderCircle class="h-5 w-5 animate-spin" />{:else}<RotateCcw class="h-5 w-5 text-warning" />{/if}
        <span><span class="block font-semibold">清除系统代理</span>
          <span class="block whitespace-normal text-xs text-muted-foreground">关闭当前 HTTP、HTTPS 和 SOCKS 代理</span></span>
      </Button>
      {#if !guiState.canEnableSystemProxy && !pending}<p class="text-xs text-muted-foreground">接管需要可用的内核和代理配置；仍可清除代理。</p>{/if}
      {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="ghost" disabled={pending !== null} onclick={() => guiState.systemProxyRecoveryOpen = false}>取消</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
