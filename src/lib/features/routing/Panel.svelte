<script lang="ts">
  import { RouteTraceState } from '$lib/features/routing/state.svelte';
  import { onDestroy } from 'svelte';
  import { Clipboard, LoaderCircle, Network } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getAppErrorMessage } from '$lib/services/core';
  import { guiTraceRoute } from './client';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import type { TraceRouteResult, TraceHop } from '$lib/types/diagnostics';

  const route = new RouteTraceState({guiTraceRoute, getAppErrorMessage});

  let traceCopyFeedback = $state<string | null>(null);
  let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

  function fmtElapsed(ms: number | undefined): string { return ms == null ? '' : `${ms}ms`; }

  function traceHops(r: TraceRouteResult): TraceHop[] {
    return r.hops ?? [];
  }

  function fmtRtt(rtt: number | number[] | undefined): string {
    if (rtt == null) return '—';
    if (Array.isArray(rtt)) return rtt.length ? rtt.map((v) => `${v}ms`).join(' / ') : '—';
    return `${rtt}ms`;
  }

  async function copyText(text: string) {
    try {
      await copyTextToClipboard(text);
      traceCopyFeedback = '已复制 JSON';
    } catch (error) {
      const message = getAppErrorMessage(error, '复制失败');
      traceCopyFeedback = message;
    }
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = setTimeout(() => {

      traceCopyFeedback = null;
      copyFeedbackTimer = null;
    }, 3_000);
  }

  function onTraceKey(e: KeyboardEvent) {
    if (e.key === 'Enter') route.runTrace();
  }

  onDestroy(() => {
    route.dispose();
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  });
</script>

  <!-- Route trace -->
  <section class="diag-tool">
    <div class="diag-head">
      <span class="diag-title">路由追踪</span>
      <span class="diag-hint">逐跳探测到目标的路径</span>
    </div>
    <div class="diag-form">
      <Input
        class="diag-input"
        placeholder="example.com"
        bind:value={route.traceTarget}
        onkeydown={onTraceKey}
        disabled={route.traceLoading}
      />
      <Input
        class="diag-input diag-input--port"
        type="number"
        placeholder="端口"
        bind:value={route.tracePort}
        disabled={route.traceLoading}
      />
      <Input
        class="diag-input diag-input--proto"
        placeholder="协议（可选）"
        bind:value={route.traceProtocol}
        disabled={route.traceLoading}
      />
      <Input
        class="diag-input diag-input--inbound"
        placeholder="入口标签（可选）"
        bind:value={route.traceInboundTag}
        disabled={route.traceLoading}
      />
      <Button size="sm" onclick={() => route.runTrace()} disabled={route.traceLoading || !route.traceTarget.trim()}>
        {#if route.traceLoading}<LoaderCircle class="animate-spin" />{:else}<Network />{/if}
        {route.traceLoading ? '追踪中…' : '追踪'}
      </Button>
    </div>
    {#if route.traceLoading}
      <div class="diag-state">追踪中…（可能需要数秒）</div>
    {:else if route.traceError}
      <div class="diag-error">{route.traceError}</div>
    {:else if route.traceResult}
      <div class="diag-result">
        <div class="diag-meta">
          {#if route.traceResult.target}<span>target {route.traceResult.target}</span>{/if}
          {#if route.traceResult.totalHops != null}<span>{route.traceResult.totalHops} hops</span>{/if}
          {#if route.traceResult.elapsedMs != null}<span>{fmtElapsed(route.traceResult.elapsedMs)}</span>{/if}
          {#if traceCopyFeedback}<span class="copy-feedback" role="status">{traceCopyFeedback}</span>{/if}
          <Button variant="ghost" size="xs" class="diag-copy" onclick={() => copyText(JSON.stringify(route.traceResult, null, 2))}><Clipboard />复制 JSON</Button>
        </div>
        {#if traceHops(route.traceResult).length > 0}
          <table class="hop-table">
            <thead>
              <tr><th>TTL</th><th>地址</th><th>RTT</th></tr>
            </thead>
            <tbody>
              {#each traceHops(route.traceResult) as hop, i}
                <tr>
                  <td class="hop-ttl">{hop.ttl ?? hop.hop ?? i + 1}</td>
                  <td class="hop-addr">{hop.timeout ? '*' : (hop.addr ?? hop.address ?? hop.ip ?? hop.host ?? '?')}</td>
                  <td class="hop-rtt">{hop.timeout ? '超时' : fmtRtt(hop.rtt ?? hop.latency)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else if route.traceResult.error}
          <div class="diag-error">{route.traceResult.error}</div>
        {:else}
          <pre class="diag-json">{JSON.stringify(route.traceResult, null, 2)}</pre>
        {/if}
      </div>
    {/if}
  </section>
<style>

  .diag-tool {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 0 16px;
    flex-shrink: 0;
  }

  .diag-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .diag-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
  }

  .diag-hint {
    font-size: 11.5px;
    color: var(--muted-foreground);
    opacity: 0.8;
  }

  .diag-form {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
  }

  :global(.diag-input) {
    height: var(--control-height);
    font-size: 12px;
    flex: 1;
    min-width: 0;
  }

  :global(.diag-input--port) {
    flex: 0 0 60px;
  }

  :global(.diag-input--proto) {
    flex: 0 0 130px;
  }

  :global(.diag-input--inbound) {
    flex: 0 0 140px;
  }

  .diag-state {
    padding: 4px 2px;
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .diag-error {
    padding: 5px 7px;
    border-radius: 4px;
    background: rgba(239, 68, 68, 0.08);
    color: var(--destructive);
    font-size: 11px;
    font-family: var(--font-mono);
    user-select: text;
    -webkit-user-select: text;
  }

  .diag-result {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .diag-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }

  :global(.diag-copy) {
    margin-left: auto;
    font-size: 10px;
  }

  .copy-feedback {
    margin-left: auto;
    color: var(--success);
    font-family: inherit;
  }

  .copy-feedback + :global(.diag-copy) {
    margin-left: 0;
  }

  .hop-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
    font-family: var(--font-mono);
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .hop-table th {
    text-align: left;
    padding: 4px 7px;
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 600;
    border-bottom: 1px solid var(--border);
  }

  .hop-table td {
    padding: 3px 7px;
    border-bottom: 1px solid var(--border);
    color: var(--foreground);
    user-select: text;
    -webkit-user-select: text;
  }

  .hop-table tr:last-child td { border-bottom: none; }

  .hop-ttl { width: 40px; color: var(--muted-foreground); }
  .hop-addr { color: var(--foreground); }
  .hop-rtt { width: 120px; color: var(--muted-foreground); }

  .diag-json {
    margin: 0;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--foreground);
    font-size: 10px;
    font-family: var(--font-mono);
    line-height: 1.45;
    overflow: auto;
    white-space: pre;
    max-height: 240px;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
