<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Clipboard, LoaderCircle, Network, Search, Settings2, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { getAppErrorMessage, guiClearFakeIp, guiDnsCache, guiDnsLookup, guiFakeIpLookup, guiTraceRoute } from '$lib/services/core';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { store } from '$lib/services/store.svelte';
  import type { DnsCacheResult, DnsLookupResult, FakeIpLookupResult, TraceRouteResult, DnsRecord, TraceHop } from '$lib/types/diagnostics';

  // DNS lookup
  let dnsHost = $state('');
  let dnsLoading = $state(false);
  let dnsResult = $state<DnsLookupResult | null>(null);
  let dnsError = $state<string | null>(null);
  let dnsCopyFeedback = $state<string | null>(null);

  let fakeQuery = $state('');
  let fakeDirection = $state<'domain' | 'ip'>('domain');
  let fakeLoading = $state(false);
  let fakeClearLoading = $state(false);
  let fakeResult = $state<FakeIpLookupResult | null>(null);
  let cacheResult = $state<DnsCacheResult | null>(null);
  let fakeMessage = $state<string | null>(null);
  let fakeError = $state<string | null>(null);

  // Route trace
  let traceTarget = $state('');
  let tracePort = $state(80);
  let traceProtocol = $state('');
  let traceInboundTag = $state('');
  let traceLoading = $state(false);
  let traceResult = $state<TraceRouteResult | null>(null);
  let traceError = $state<string | null>(null);
  let traceCopyFeedback = $state<string | null>(null);
  let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

  async function runDns() {
    const host = dnsHost.trim();
    if (!host || dnsLoading) return;
    dnsLoading = true;
    dnsError = null;
    dnsResult = null;
    try {
      dnsResult = await guiDnsLookup(host);
    } catch (e) {
      dnsError = getAppErrorMessage(e, 'DNS 查询失败');
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
      const inboundTag = traceInboundTag.trim() || undefined;
      traceResult = await guiTraceRoute(target, tracePort || undefined, proto, inboundTag);
    } catch (e) {
      traceError = getAppErrorMessage(e, '路由追踪失败');
    } finally {
      traceLoading = false;
    }
  }

  async function runFakeIp() {
    const query = fakeQuery.trim();
    if (!query || fakeLoading) return;
    fakeLoading = true;
    fakeError = null;
    fakeMessage = null;
    try {
      [fakeResult, cacheResult] = await Promise.all([
        guiFakeIpLookup(fakeDirection === 'domain' ? { domain: query } : { ip: query }),
        guiDnsCache(undefined, 50),
      ]);
    } catch (e) {
      fakeError = getAppErrorMessage(e, 'Fake-IP 诊断失败');
    } finally {
      fakeLoading = false;
    }
  }

  async function clearFakeIp(scope: 'selected' | 'all') {
    if (fakeClearLoading || fakeLoading) return;
    const query = fakeQuery.trim();
    if (scope === 'selected' && !query) {
      fakeError = '请先填写要删除的域名或 Fake-IP';
      return;
    }
    if (scope === 'all' && !confirm('确认清空全部 Fake-IP 映射？\n\n应用可能仍缓存旧的虚拟地址，受影响连接需要重新解析 DNS。')) {
      return;
    }

    fakeClearLoading = true;
    fakeError = null;
    fakeMessage = null;
    try {
      const result = await guiClearFakeIp(
        scope === 'all'
          ? undefined
          : fakeDirection === 'domain'
            ? { domain: query }
            : { ip: query },
      );
      fakeMessage = result.enabled
        ? `已删除 ${result.removedMappings} 个映射（${result.removedAddresses} 个地址），剩余 ${result.liveMappings} 个，隔离地址 ${result.retiredAddresses} 个`
        : '当前配置未启用 Fake-IP，没有可清理的映射';
      fakeResult = null;
      cacheResult = await guiDnsCache(undefined, 50);
    } catch (e) {
      fakeError = getAppErrorMessage(e, '清理 Fake-IP 缓存失败');
    } finally {
      fakeClearLoading = false;
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

  async function copyText(text: string, target: 'dns' | 'trace') {
    try {
      await copyTextToClipboard(text);
      if (target === 'dns') dnsCopyFeedback = '已复制 JSON';
      else traceCopyFeedback = '已复制 JSON';
    } catch (error) {
      const message = getAppErrorMessage(error, '复制失败');
      if (target === 'dns') dnsCopyFeedback = message;
      else traceCopyFeedback = message;
    }
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = setTimeout(() => {
      dnsCopyFeedback = null;
      traceCopyFeedback = null;
      copyFeedbackTimer = null;
    }, 3_000);
  }

  function onDnsKey(e: KeyboardEvent) {
    if (e.key === 'Enter') runDns();
  }

  function onTraceKey(e: KeyboardEvent) {
    if (e.key === 'Enter') runTrace();
  }

  onDestroy(() => {
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  });
</script>

<div class="diag-panel">
  <!-- DNS lookup -->
  <section class="diag-tool">
    <div class="diag-head">
      <span class="diag-title">域名查询</span>
      <span class="diag-hint">解析域名记录（A / AAAA / CNAME / MX …）</span>
    </div>
    <div class="diag-form">
      <Input
        class="diag-input"
        placeholder="example.com"
        bind:value={dnsHost}
        onkeydown={onDnsKey}
        disabled={dnsLoading}
      />
      <Button size="sm" onclick={runDns} disabled={dnsLoading || !dnsHost.trim()}>
        {#if dnsLoading}<LoaderCircle class="animate-spin" />{:else}<Search />{/if}
        {dnsLoading ? '查询中…' : '查询'}
      </Button>
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
          {#if dnsCopyFeedback}<span class="copy-feedback" role="status">{dnsCopyFeedback}</span>{/if}
          <Button variant="ghost" size="xs" class="diag-copy" onclick={() => copyText(JSON.stringify(dnsResult, null, 2), 'dns')}><Clipboard />复制 JSON</Button>
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
        {:else if dnsResult.resolved_addresses?.length}
          <div class="dns-list">
            {#each dnsResult.resolved_addresses as address}
              <div class="dns-rec"><span class="dns-type">IP</span><span class="dns-name">{dnsResult.hostname ?? dnsHost}</span><span class="dns-value">{address}</span></div>
            {/each}
          </div>
        {:else if dnsResult.error}
          <div class="diag-error">{dnsResult.error}</div>
        {:else}
          <pre class="diag-json">{JSON.stringify(dnsResult, null, 2)}</pre>
        {/if}
        {#if dnsResult.attempts?.length}
          <div class="dns-attempts">
            <strong>DNS 后端尝试 · {dnsResult.query_role ?? 'default'}</strong>
            {#each dnsResult.attempts as attempt, index}
              <div class:failed={attempt.success === false}>
                <span>#{index + 1} {attempt.server_tag ?? 'unknown'} · {attempt.transport ?? '?'}</span>
                <small>{attempt.server_endpoints?.join(' · ') || '无具体端点'}{attempt.outbound ? ` · outbound ${attempt.outbound}` : ''}</small>
                <small>{attempt.success ? '成功' : attempt.failure_reason || '失败'}</small>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </section>

  <section class="diag-tool">
    <div class="diag-head-row">
      <div class="diag-head">
        <span class="diag-title">解析缓存</span>
        <span class="diag-hint">查询映射与运行计数，也可删除指定映射或清空全部缓存</span>
      </div>
      <Button
        variant="outline"
        size="sm"
        class="fake-config-button"
        onclick={() => store.openSettings('dns')}
        title="管理内核 Fake-IP 配置"
      >
        <Settings2 />管理内核配置
      </Button>
    </div>
    <div class="diag-form">
      <Select.Root
        type="single"
        value={fakeDirection}
        disabled={fakeLoading || fakeClearLoading}
        onValueChange={(value) => {
          if (value === 'domain' || value === 'ip') fakeDirection = value;
        }}
      >
        <Select.Trigger class="diag-select" aria-label="Fake-IP 查询方向">
          {fakeDirection === 'domain' ? '域名 → Fake-IP' : 'Fake-IP → 域名'}
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="domain" label="域名 → Fake-IP">域名 → Fake-IP</Select.Item>
          <Select.Item value="ip" label="Fake-IP → 域名">Fake-IP → 域名</Select.Item>
        </Select.Content>
      </Select.Root>
      <Input class="diag-input" placeholder={fakeDirection === 'domain' ? 'open.bigmodel.cn' : '198.18.0.2'} bind:value={fakeQuery} onkeydown={(event) => event.key === 'Enter' && runFakeIp()} disabled={fakeLoading || fakeClearLoading} />
      <Button size="sm" onclick={runFakeIp} disabled={fakeLoading || fakeClearLoading || !fakeQuery.trim()}>
        {#if fakeLoading}<LoaderCircle class="animate-spin" />{:else}<Search />{/if}{fakeLoading ? '查询中…' : '查询'}
      </Button>
      <Button variant="outline" size="sm" onclick={() => clearFakeIp('selected')} disabled={fakeLoading || fakeClearLoading || !fakeQuery.trim()}>
        {#if fakeClearLoading}<LoaderCircle class="animate-spin" />{:else}<Trash2 />{/if}删除当前映射
      </Button>
      <Button variant="destructive" size="sm" onclick={() => clearFakeIp('all')} disabled={fakeLoading || fakeClearLoading}>
        {#if fakeClearLoading}<LoaderCircle class="animate-spin" />{:else}<Trash2 />{/if}清空全部
      </Button>
    </div>
    {#if fakeError}<div class="diag-error">{fakeError}</div>{/if}
    {#if fakeMessage}<div class="diag-success" role="status">{fakeMessage}</div>{/if}
    {#if fakeResult}
      <div class="fake-summary">
        <div><span>状态</span><strong>{fakeResult.enabled ? '已启用' : '未启用'}</strong></div>
        <div><span>查询</span><strong>{fakeResult.domain ?? fakeResult.ip ?? fakeQuery}</strong></div>
        <div><span>结果</span><strong>{fakeResult.fake_ip ?? fakeResult.domain ?? '未命中'}</strong></div>
        {#if fakeResult.stats}
          <div><span>Live / Retired / Capacity</span><strong>{fakeResult.stats.live_mappings} / {fakeResult.stats.retired_addresses ?? 0} / {fakeResult.stats.capacity}</strong></div>
          <div><span>分配 / 过期 / 驱逐</span><strong>{fakeResult.stats.allocations} / {fakeResult.stats.expirations} / {fakeResult.stats.evictions}</strong></div>
          <div><span>耗尽 / 冲突 / Reverse miss</span><strong>{fakeResult.stats.exhaustions} / {fakeResult.stats.collisions} / {fakeResult.stats.reverse_misses}</strong></div>
        {/if}
      </div>
    {/if}
    {#if cacheResult}
      <div class="diag-meta"><span>DNS cache {cacheResult.enabled ? 'enabled' : 'disabled'}</span><span>{cacheResult.count ?? cacheResult.entries?.length ?? 0} entries</span></div>
      {#if cacheResult.entries?.length}
        <div class="dns-list">
          {#each cacheResult.entries as entry}
            <div class="dns-rec"><span class="dns-type">CACHE</span><span class="dns-name">{entry.domain}</span><span class="dns-value">{entry.addresses.join(', ')}</span>{#if entry.ttl_seconds != null}<span class="dns-ttl">ttl {entry.ttl_seconds}</span>{/if}</div>
          {/each}
        </div>
      {/if}
    {/if}
  </section>

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
        bind:value={traceTarget}
        onkeydown={onTraceKey}
        disabled={traceLoading}
      />
      <Input
        class="diag-input diag-input--port"
        type="number"
        placeholder="端口"
        bind:value={tracePort}
        disabled={traceLoading}
      />
      <Input
        class="diag-input diag-input--proto"
        placeholder="协议（可选）"
        bind:value={traceProtocol}
        disabled={traceLoading}
      />
      <Input
        class="diag-input diag-input--inbound"
        placeholder="入口标签（可选）"
        bind:value={traceInboundTag}
        disabled={traceLoading}
      />
      <Button size="sm" onclick={runTrace} disabled={traceLoading || !traceTarget.trim()}>
        {#if traceLoading}<LoaderCircle class="animate-spin" />{:else}<Network />{/if}
        {traceLoading ? '追踪中…' : '追踪'}
      </Button>
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
          {#if traceCopyFeedback}<span class="copy-feedback" role="status">{traceCopyFeedback}</span>{/if}
          <Button variant="ghost" size="xs" class="diag-copy" onclick={() => copyText(JSON.stringify(traceResult, null, 2), 'trace')}><Clipboard />复制 JSON</Button>
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
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .diag-tool {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 0 16px;
    flex-shrink: 0;
  }

  .diag-tool + .diag-tool {
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .diag-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .diag-head-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  :global(.fake-config-button) {
    flex: 0 0 auto;
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

  :global(.diag-select) {
    width: 156px;
    flex: 0 0 156px;
  }

  .fake-summary {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }

  .fake-summary > div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
    padding: 7px;
    border: 1px solid var(--border);
    border-radius: 5px;
  }

  .fake-summary span { color: var(--muted-foreground); font-size: 10px; }
  .fake-summary strong { overflow-wrap: anywhere; font-family: var(--font-mono); font-size: 11px; }

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

  .diag-success {
    padding: 5px 7px;
    border-radius: 4px;
    background: rgba(34, 197, 94, 0.08);
    color: var(--success);
    font-size: 11px;
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

  .dns-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .dns-attempts { display: flex; flex-direction: column; gap: 5px; margin-top: 9px; }
  .dns-attempts > strong { font-size: 10.5px; }
  .dns-attempts > div { padding: 7px 8px; border: 1px solid var(--border); border-radius: 6px; background: color-mix(in srgb, var(--muted) 20%, transparent); }
  .dns-attempts > div.failed { border-color: rgba(239, 68, 68, .28); }
  .dns-attempts span, .dns-attempts small { display: block; overflow-wrap: anywhere; }
  .dns-attempts span { font-family: var(--font-mono); font-size: 10.5px; }
  .dns-attempts small { margin-top: 2px; color: var(--muted-foreground); font-size: 9.5px; }

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
