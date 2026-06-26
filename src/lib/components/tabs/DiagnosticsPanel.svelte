<script lang="ts">
  import { guiDnsLookup, guiTraceRoute } from '$lib/services/core';
  import type { DnsLookupResult, TraceRouteResult, DnsRecord, TraceHop } from '$lib/types/diagnostics';

  // DNS lookup
  let dnsHost = $state('');
  let dnsLoading = $state(false);
  let dnsResult = $state<DnsLookupResult | null>(null);
  let dnsError = $state<string | null>(null);

  // Route trace
  let traceTarget = $state('');
  let tracePort = $state(80);
  let traceProtocol = $state('');
  let traceLoading = $state(false);
  let traceResult = $state<TraceRouteResult | null>(null);
  let traceError = $state<string | null>(null);

  async function runDns() {
    const host = dnsHost.trim();
    if (!host || dnsLoading) return;
    dnsLoading = true;
    dnsError = null;
    dnsResult = null;
    try {
      dnsResult = await guiDnsLookup(host);
    } catch (e) {
      dnsError = e instanceof Error ? e.message : String(e);
    } finally {
      dnsLoading = false;
    }
  }

  async function runTrace() {
    const target = traceTarget.trim();
    if (!target || traceLoading) return;
    traceLoading = true;
    traceError = null;
    traceResult = null;
    try {
      const proto = traceProtocol.trim() || undefined;
      traceResult = await guiTraceRoute(target, tracePort || undefined, proto);
    } catch (e) {
      traceError = e instanceof Error ? e.message : String(e);
    } finally {
      traceLoading = false;
    }
  }

  // The kernel may carry records under any of several field names.
  function dnsRecords(r: DnsLookupResult): DnsRecord[] {
    return r.answers ?? r.records ?? r.results ?? [];
  }

  function traceHops(r: TraceRouteResult): TraceHop[] {
    return r.hops ?? [];
  }

  function fmtRtt(rtt: number | number[] | undefined): string {
    if (rtt == null) return '—';
    if (Array.isArray(rtt)) return rtt.length ? rtt.map((v) => `${v}ms`).join(' / ') : '—';
    return `${rtt}ms`;
  }

  function fmtElapsed(ms: number | undefined): string {
    return ms == null ? '' : `${ms}ms`;
  }

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard unavailable — ignore */
    }
  }

  function onDnsKey(e: KeyboardEvent) {
    if (e.key === 'Enter') runDns();
  }

  function onTraceKey(e: KeyboardEvent) {
    if (e.key === 'Enter') runTrace();
  }
</script>

<div class="diag-panel">
  <!-- DNS lookup -->
  <section class="diag-tool">
    <div class="diag-head">
      <span class="diag-title">DNS 查询</span>
      <span class="diag-hint">解析域名记录（A / AAAA / CNAME / MX …）</span>
    </div>
    <div class="diag-form">
      <input
        class="diag-input"
        placeholder="example.com"
        bind:value={dnsHost}
        onkeydown={onDnsKey}
        disabled={dnsLoading}
      />
      <button class="diag-btn" onclick={runDns} disabled={dnsLoading || !dnsHost.trim()}>
        {dnsLoading ? '查询中…' : '查询'}
      </button>
    </div>
    {#if dnsLoading}
      <div class="diag-state">查询中…</div>
    {:else if dnsError}
      <div class="diag-error">{dnsError}</div>
    {:else if dnsResult}
      <div class="diag-result">
        <div class="diag-meta">
          {#if dnsResult.rcode != null}<span>rcode {dnsResult.rcode}</span>{/if}
          {#if dnsResult.server}<span>server {dnsResult.server}</span>{/if}
          {#if dnsResult.elapsedMs != null}<span>{fmtElapsed(dnsResult.elapsedMs)}</span>{/if}
          <button class="diag-copy" onclick={() => copyText(JSON.stringify(dnsResult, null, 2))}>复制 JSON</button>
        </div>
        {#if dnsRecords(dnsResult).length > 0}
          <div class="dns-list">
            {#each dnsRecords(dnsResult) as rec}
              <div class="dns-rec">
                <span class="dns-type">{rec.type ?? '?'}</span>
                <span class="dns-name">{rec.name ?? dnsResult.hostname ?? ''}</span>
                <span class="dns-value">{rec.value ?? rec.data ?? ''}</span>
                {#if rec.ttl != null}<span class="dns-ttl">ttl {rec.ttl}</span>{/if}
              </div>
            {/each}
          </div>
        {:else if dnsResult.error}
          <div class="diag-error">{dnsResult.error}</div>
        {:else}
          <pre class="diag-json">{JSON.stringify(dnsResult, null, 2)}</pre>
        {/if}
      </div>
    {/if}
  </section>

  <!-- Route trace -->
  <section class="diag-tool">
    <div class="diag-head">
      <span class="diag-title">路由追踪</span>
      <span class="diag-hint">逐跳探测到目标的路径</span>
    </div>
    <div class="diag-form">
      <input
        class="diag-input"
        placeholder="example.com"
        bind:value={traceTarget}
        onkeydown={onTraceKey}
        disabled={traceLoading}
      />
      <input
        class="diag-input diag-input--port"
        type="number"
        placeholder="端口"
        bind:value={tracePort}
        disabled={traceLoading}
      />
      <input
        class="diag-input diag-input--proto"
        placeholder="协议 (tcp/udp/icmp)"
        bind:value={traceProtocol}
        disabled={traceLoading}
      />
      <button class="diag-btn" onclick={runTrace} disabled={traceLoading || !traceTarget.trim()}>
        {traceLoading ? '追踪中…' : '追踪'}
      </button>
    </div>
    {#if traceLoading}
      <div class="diag-state">追踪中…（可能需要数秒）</div>
    {:else if traceError}
      <div class="diag-error">{traceError}</div>
    {:else if traceResult}
      <div class="diag-result">
        <div class="diag-meta">
          {#if traceResult.target}<span>target {traceResult.target}</span>{/if}
          {#if traceResult.totalHops != null}<span>{traceResult.totalHops} hops</span>{/if}
          {#if traceResult.elapsedMs != null}<span>{fmtElapsed(traceResult.elapsedMs)}</span>{/if}
          <button class="diag-copy" onclick={() => copyText(JSON.stringify(traceResult, null, 2))}>复制 JSON</button>
        </div>
        {#if traceHops(traceResult).length > 0}
          <table class="hop-table">
            <thead>
              <tr><th>TTL</th><th>地址</th><th>RTT</th></tr>
            </thead>
            <tbody>
              {#each traceHops(traceResult) as hop, i}
                <tr>
                  <td class="hop-ttl">{hop.ttl ?? hop.hop ?? i + 1}</td>
                  <td class="hop-addr">{hop.timeout ? '*' : (hop.addr ?? hop.address ?? hop.ip ?? hop.host ?? '?')}</td>
                  <td class="hop-rtt">{hop.timeout ? '超时' : fmtRtt(hop.rtt ?? hop.latency)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else if traceResult.error}
          <div class="diag-error">{traceResult.error}</div>
        {:else}
          <pre class="diag-json">{JSON.stringify(traceResult, null, 2)}</pre>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .diag-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .diag-tool {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--card);
    flex-shrink: 0;
  }

  .diag-head {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .diag-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--foreground);
  }

  .diag-hint {
    font-size: 10px;
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .diag-form {
    display: flex;
    gap: 5px;
  }

  .diag-input {
    height: 24px;
    padding: 0 7px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--background);
    color: var(--foreground);
    font-size: 11px;
    flex: 1;
    min-width: 0;
  }

  .diag-input:focus {
    outline: none;
    border-color: var(--primary);
  }

  .diag-input--port {
    flex: 0 0 60px;
  }

  .diag-input--proto {
    flex: 0 0 130px;
  }

  .diag-btn {
    height: 24px;
    padding: 0 12px;
    border-radius: 5px;
    border: none;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    flex-shrink: 0;
    transition: opacity 0.12s ease;
  }

  .diag-btn:hover:not(:disabled) { opacity: 0.88; }
  .diag-btn:disabled { opacity: 0.45; cursor: not-allowed; }

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

  .diag-copy {
    margin-left: auto;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--muted-foreground);
    font-size: 10px;
    padding: 1px 7px;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .diag-copy:hover {
    color: var(--foreground);
    background: var(--muted);
  }

  .dns-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .dns-rec {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 7px;
    font-size: 11px;
    border-bottom: 1px solid var(--border);
  }

  .dns-rec:last-child { border-bottom: none; }

  .dns-type {
    font-family: var(--font-mono);
    font-weight: 700;
    color: var(--primary);
    min-width: 44px;
    flex-shrink: 0;
  }

  .dns-name {
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    flex-shrink: 0;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dns-value {
    flex: 1;
    font-family: var(--font-mono);
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
    -webkit-user-select: text;
  }

  .dns-ttl {
    font-size: 10px;
    color: var(--muted-foreground);
    flex-shrink: 0;
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
