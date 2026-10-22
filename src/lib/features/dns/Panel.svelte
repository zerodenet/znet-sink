<script lang="ts">
  import { DnsState } from '$lib/features/dns/state.svelte';
  import { onDestroy } from 'svelte';
  import { Clipboard, LoaderCircle, Search, Settings2, Trash2, XCircle } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import ToolJobHistory from '$lib/features/tool-jobs/History.svelte';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { getAppErrorMessage } from '$lib/services/core';
  import {
    cancelToolJob,
    listToolJobs,
    startToolJob,
    subscribeToolJobs,
    getToolScope,
    subscribeToolScope,
  } from '$lib/features/tool-jobs/client';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { store } from '$lib/services/store.svelte';
  import type { DnsLookupResult, DnsRecord } from '$lib/types/diagnostics';

  const dns = new DnsState({
    start: startToolJob,
    list: listToolJobs,
    cancel: cancelToolJob,
    subscribe: subscribeToolJobs,
    scope: getToolScope,
    subscribeScope: subscribeToolScope,
    getAppErrorMessage,
    confirm: (message) => window.confirm(message),
  });

  let dnsCopyFeedback = $state<string | null>(null);

  let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

  // The kernel may carry records under any of several field names.
  function dnsRecords(r: DnsLookupResult): DnsRecord[] {
    return r.answers ?? r.records ?? r.results ?? [];
  }

  function fmtElapsed(ms: number | undefined): string {
    return ms == null ? '' : `${ms}ms`;
  }

  function jobLabel(kind: string): string {
    return ({ dns_lookup: '域名查询', dns_cache: '缓存读取', fake_ip_lookup: 'Fake-IP 查询', fake_ip_clear: 'Fake-IP 清理' } as Record<string, string>)[kind] ?? kind;
  }

  async function copyText(text: string) {
    try {
      await copyTextToClipboard(text);
      dnsCopyFeedback = '已复制 JSON';

    } catch (error) {
      const message = getAppErrorMessage(error, '复制失败');
      dnsCopyFeedback = message;

    }
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = setTimeout(() => {
      dnsCopyFeedback = null;

      copyFeedbackTimer = null;
    }, 3_000);
  }

  function onDnsKey(e: KeyboardEvent) {
    if (e.key === 'Enter') dns.runDns();
  }

  onDestroy(() => {
    dns.dispose();
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  });
</script>

  <ToolJobHistory jobs={dns.jobs} />

  {#if dns.jobs.active().length > 0}
    <div class="job-strip" aria-label="DNS 与 Fake-IP 活动任务">
      {#each dns.jobs.active() as job (job.id)}
        <div class="job-item">
          <LoaderCircle class="animate-spin" />
          <span>#{job.id} · {jobLabel(job.kind)} · {job.subject}</span>
          <small>{job.state === 'queued' ? '排队中' : job.state === 'cancelling' ? '正在停止' : '执行中'}</small>
          <Button variant="ghost" size="xs" onclick={() => dns.cancel(job.id)} title="停止任务"><XCircle />停止</Button>
        </div>
      {/each}
    </div>
  {/if}

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
        bind:value={dns.dnsHost}
        onkeydown={onDnsKey}
        disabled={dns.dnsLoading}
      />
      <Button size="sm" onclick={() => dns.runDns()} disabled={!dns.jobs.ready || dns.dnsLoading || !dns.dnsHost.trim()}>
        {#if dns.dnsLoading}<LoaderCircle class="animate-spin" />{:else}<Search />{/if}
        {dns.dnsLoading ? '查询中…' : '查询'}
      </Button>
    </div>
    {#if dns.dnsLoading}
      <div class="diag-state">查询中…</div>
    {:else if dns.dnsError}
      <div class="diag-error">{dns.dnsError}</div>
    {:else if dns.dnsResult}
      <div class="diag-result">
        <div class="diag-meta">
          {#if dns.dnsResult.rcode != null}<span>rcode {dns.dnsResult.rcode}</span>{/if}
          {#if dns.dnsResult.server}<span>server {dns.dnsResult.server}</span>{/if}
          {#if dns.dnsResult.elapsedMs != null}<span>{fmtElapsed(dns.dnsResult.elapsedMs)}</span>{/if}
          {#if dnsCopyFeedback}<span class="copy-feedback" role="status">{dnsCopyFeedback}</span>{/if}
          <Button variant="ghost" size="xs" class="diag-copy" onclick={() => copyText(JSON.stringify(dns.dnsResult, null, 2))}><Clipboard />复制 JSON</Button>
        </div>
        {#if dnsRecords(dns.dnsResult).length > 0}
          <div class="dns-list">
            {#each dnsRecords(dns.dnsResult) as rec}
              <div class="dns-rec">
                <span class="dns-type">{rec.type ?? '?'}</span>
                <span class="dns-name">{rec.name ?? dns.dnsResult.hostname ?? ''}</span>
                <span class="dns-value">{rec.value ?? rec.data ?? ''}</span>
                {#if rec.ttl != null}<span class="dns-ttl">ttl {rec.ttl}</span>{/if}
              </div>
            {/each}
          </div>
        {:else if dns.dnsResult.resolved_addresses?.length}
          <div class="dns-list">
            {#each dns.dnsResult.resolved_addresses as address}
              <div class="dns-rec"><span class="dns-type">IP</span><span class="dns-name">{dns.dnsResult.hostname ?? dns.dnsHost}</span><span class="dns-value">{address}</span></div>
            {/each}
          </div>
        {:else if dns.dnsResult.error}
          <div class="diag-error">{dns.dnsResult.error}</div>
        {:else}
          <pre class="diag-json">{JSON.stringify(dns.dnsResult, null, 2)}</pre>
        {/if}
        {#if dns.dnsResult.attempts?.length}
          <div class="dns-attempts">
            <strong>DNS 后端尝试 · {dns.dnsResult.query_role ?? 'default'}</strong>
            {#each dns.dnsResult.attempts as attempt, index}
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

  <section class="diag-tool cache-tool" aria-label="DNS 缓存">
    <div class="diag-head"><h2 class="diag-title">DNS 缓存</h2><p class="diag-hint">独立读取内核解析缓存，不进行 DNS 查询、不分配 Fake-IP。留空域名读取有界列表。</p></div>
    <div class="diag-form">
      <label>域名（可选）<Input aria-label="缓存域名" placeholder="留空列出缓存" bind:value={dns.cacheDomain} disabled={dns.cacheLoading} /></label>
      <label>列表上限（1–500）<Input aria-label="缓存列表上限" type="number" min="1" max="500" step="1" bind:value={dns.cacheLimit} disabled={dns.cacheLoading} /></label>
      <Button onclick={() => dns.runCache()} disabled={!dns.jobs.ready || dns.cacheLoading}>{dns.cacheLoading ? '读取中…' : '读取缓存'}</Button>
    </div>
    {#if dns.cacheError}<div class="diag-error" role="status">{dns.cacheError}</div>{/if}
    {#if dns.cacheResult}
      {@const cache = dns.cacheResult}
      <div class="cache-facts" aria-label="当前 DNS 缓存结果">
        <p>{cache.enabled ? '内核缓存已启用' : '内核缓存未启用'}</p>
        {#if cache.domain !== undefined}
          <p>域名：{cache.domain}</p><p>{cache.hit ? '命中缓存' : '未命中缓存；没有发起新解析'}</p>
          {#if cache.hit}<p>地址：{cache.addresses?.join(' · ') || '未提供'}</p><p>剩余 TTL：{cache.ttl_seconds ?? '—'} 秒</p>{/if}
        {:else}
          <p>本次返回 {cache.count ?? '—'} 项；仅表示当前上限内的实时列表，不是缓存总量或完整历史。</p>
          {#if cache.entries?.length === 0}<p>本次读取没有缓存条目</p>{/if}
          {#each cache.entries ?? [] as entry}
            <dl><dt>域名</dt><dd>{entry.domain}</dd><dt>地址</dt><dd>{entry.addresses.join(' · ') || '未提供'}</dd><dt>剩余 TTL</dt><dd>{entry.ttl_seconds ?? '—'} 秒</dd></dl>
          {/each}
        {/if}
      </div>
    {/if}
  </section>

  <section class="diag-tool">
    <div class="diag-head-row">
      <div class="diag-head">
        <span class="diag-title">Fake-IP 映射</span>
        <span class="diag-hint">查看已有映射；清理只影响内核 Fake-IP，不会清除外部应用的 DNS 缓存</span>
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
        value={dns.fakeDirection}
        disabled={dns.fakeLoading || dns.fakeClearLoading}
        onValueChange={(value) => {
          if (value === 'domain' || value === 'ip') dns.fakeDirection = value;
        }}
      >
        <Select.Trigger class="diag-select" aria-label="Fake-IP 查询方向">
          {dns.fakeDirection === 'domain' ? '域名 → Fake-IP' : 'Fake-IP → 域名'}
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="domain" label="域名 → Fake-IP">域名 → Fake-IP</Select.Item>
          <Select.Item value="ip" label="Fake-IP → 域名">Fake-IP → 域名</Select.Item>
        </Select.Content>
      </Select.Root>
      <Input class="diag-input" placeholder={dns.fakeDirection === 'domain' ? 'open.bigmodel.cn' : '198.18.0.2'} bind:value={dns.fakeQuery} onkeydown={(event) => event.key === 'Enter' && dns.runFakeIp()} disabled={dns.fakeLoading || dns.fakeClearLoading} />
      <Button size="sm" onclick={() => dns.runFakeIp()} disabled={!dns.jobs.ready || dns.fakeLoading || dns.fakeClearLoading || !dns.fakeQuery.trim()}>
        {#if dns.fakeLoading}<LoaderCircle class="animate-spin" />{:else}<Search />{/if}{dns.fakeLoading ? '查询中…' : '查询'}
      </Button>
      <Button variant="outline" size="sm" onclick={() => dns.clearFakeIp('selected')} disabled={!dns.jobs.ready || dns.fakeLoading || dns.fakeClearLoading || !dns.fakeQuery.trim()}>
        {#if dns.fakeClearLoading}<LoaderCircle class="animate-spin" />{:else}<Trash2 />{/if}删除当前映射
      </Button>
      <Button variant="destructive" size="sm" onclick={() => dns.clearFakeIp('all')} disabled={!dns.jobs.ready || dns.fakeLoading || dns.fakeClearLoading}>
        {#if dns.fakeClearLoading}<LoaderCircle class="animate-spin" />{:else}<Trash2 />{/if}清空全部
      </Button>
    </div>
    {#if dns.fakeError}<div class="diag-error">{dns.fakeError}</div>{/if}
    {#if dns.fakeMessage}<div class="diag-success" role="status">{dns.fakeMessage}</div>{/if}
    {#if dns.fakeResult}
      <div class="fake-summary">
        <div><span>状态</span><strong>{dns.fakeResult.enabled ? '已启用' : '未启用'}</strong></div>
        <div><span>查询</span><strong>{dns.fakeResult.domain ?? dns.fakeResult.ip ?? dns.fakeQuery}</strong></div>
        <div><span>结果</span><strong>{dns.fakeResult.fake_ip ?? dns.fakeResult.domain ?? '未命中'}</strong></div>
        {#if dns.fakeResult.stats}
          <div><span>Live / Retired / Capacity</span><strong>{dns.fakeResult.stats.live_mappings} / {dns.fakeResult.stats.retired_addresses ?? 0} / {dns.fakeResult.stats.capacity}</strong></div>
          <div><span>分配 / 过期 / 驱逐</span><strong>{dns.fakeResult.stats.allocations} / {dns.fakeResult.stats.expirations} / {dns.fakeResult.stats.evictions}</strong></div>
          <div><span>耗尽 / 冲突 / Reverse miss</span><strong>{dns.fakeResult.stats.exhaustions} / {dns.fakeResult.stats.collisions} / {dns.fakeResult.stats.reverse_misses}</strong></div>
        {/if}
      </div>
    {/if}

  </section>

<style>

  .job-strip {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px;
    border: 1px solid color-mix(in srgb, var(--primary) 30%, var(--border));
    border-radius: 6px;
    background: color-mix(in srgb, var(--primary) 5%, transparent);
  }

  .job-item {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: 11px;
  }

  .job-item > :global(svg) { width: 13px; height: 13px; flex: 0 0 auto; }
  .job-item > span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .job-item > small { margin-left: auto; color: var(--muted-foreground); white-space: nowrap; }

  .diag-tool {
    min-width: 0;
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
    flex-wrap: wrap;
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
    min-width: 0;
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
    flex-wrap: wrap;
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
    flex-wrap: wrap;
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
  .cache-tool label { display:flex;flex-direction:column;gap:6px;min-width:0;flex:1 1 150px;font-size:12px; }
  .cache-facts { font-size:12px;overflow-wrap:anywhere; }
  .cache-facts dl { display:grid;grid-template-columns:75px minmax(0,1fr);gap:7px;border-top:1px solid var(--border);padding:10px 0; }
  .cache-facts dd { min-width:0;overflow-wrap:anywhere; }
  .diag-tool :global(button), .diag-tool :global(input), .job-strip :global(button) { min-height:44px; }
  @media(max-width:360px) { .fake-summary { grid-template-columns:minmax(0,1fr); } .dns-name { max-width:100%; } }
</style>
