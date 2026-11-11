<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { endpointStateLabel } from '$lib/features/endpoints/policy';
  import type { NetworkEndpoint } from '$lib/features/endpoints/types';

  let { endpoint, stale = false }: { endpoint: NetworkEndpoint; stale?: boolean } = $props();
  let open = $state(false);
  let snapshot = $state<NetworkEndpoint | null>(null);
  let snapshotStale = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let fallbackFocus: HTMLElement | null = null;
  const hasWarning = $derived(!!endpoint.last_error || endpoint.health === 'degraded');
  function changeOpen(next: boolean) {
    if (next) {
      fallbackFocus = trigger?.closest('article')?.querySelector<HTMLElement>('.options summary') ?? null;
      // Polling must not erase or replace the error while someone is reading it.
      snapshot = { ...endpoint, last_error: endpoint.last_error ? { ...endpoint.last_error } : null };
      snapshotStale = stale;
    }
    open = next;
  }
  function restoreFocus(event: Event) {
    if (!hasWarning && fallbackFocus?.isConnected) {
      event.preventDefault();
      fallbackFocus.focus();
    }
  }
</script>

<Dialog.Root {open} onOpenChange={changeOpen}>
  {#if hasWarning || open}
    <Dialog.Trigger>
      {#snippet child({ props })}
        <Button {...props} bind:ref={trigger} variant="ghost" size="xs" class="h-[26px] gap-1 px-1.5 text-destructive hover:text-destructive" aria-label={`${endpoint.tag} 查看警告`}>
          <AlertTriangle class="size-3.5" /><span>查看警告</span>
        </Button>
      {/snippet}
    </Dialog.Trigger>
  {/if}
  <Dialog.Content class="sm:max-w-[560px]" onCloseAutoFocus={restoreFocus}>
    <Dialog.Header>
      <Dialog.Title>{snapshot?.tag ?? endpoint.tag} · 端点警告</Dialog.Title>
      <Dialog.Description>显示打开时的观测快照。运行中表示端点已启动，不保证连接或握手正常。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="grid min-w-0 gap-4">
      {#if snapshot}
        <p class="text-xs text-muted-foreground">运行状态：{endpointStateLabel(snapshot)} · {snapshot.health === 'degraded' ? '健康降级' : snapshot.health === 'healthy' ? '健康' : '健康未知'}</p>
        {#if snapshotStale}<p role="status" class="text-xs text-muted-foreground">当前显示上次观测，尚未取得最新状态。</p>{/if}
        {#if snapshot.last_error}
          <section class="grid min-w-0 gap-2" aria-label="最近一次错误">
            <h3 class="text-sm font-medium">最近一次错误</h3>
            <p class="whitespace-pre-wrap text-sm [overflow-wrap:anywhere]">{snapshot.last_error.message.trim() ? snapshot.last_error.message : '内核未提供错误说明。'}</p>
            {#if snapshot.last_error.code}<div class="min-w-0 text-xs"><span class="text-muted-foreground">错误代码：</span><code class="whitespace-pre-wrap [overflow-wrap:anywhere]">{snapshot.last_error.code}</code></div>{/if}
            {#if snapshot.last_error.cause}<div class="grid min-w-0 gap-1 text-xs"><span class="text-muted-foreground">底层原因</span><p class="whitespace-pre-wrap [overflow-wrap:anywhere]">{snapshot.last_error.cause}</p></div>{/if}
          </section>
        {:else}
          <p class="text-sm">内核报告端点健康降级，但未提供具体错误原因。</p>
          <p class="text-xs text-muted-foreground">可到日志页按端点名称「{snapshot.tag}」查找相关记录。</p>
        {/if}
        <p class="text-xs text-muted-foreground">观测时间：{new Date(snapshot.observed_at_unix_ms).toLocaleString()}</p>
      {/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => changeOpen(false)}>知道了</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
