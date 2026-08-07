<script lang="ts">
  import type { DisplayConnection } from '$lib/services/connection-view';
  import { AlertTriangle, X } from '@lucide/svelte';

  let {
    connection,
    canTerminate = false,
    terminating = false,
    onclose,
    onrequestterminate,
  } = $props<{
    connection: DisplayConnection | null;
    canTerminate?: boolean;
    terminating?: boolean;
    onclose: () => void;
    onrequestterminate?: (connection: DisplayConnection) => void;
  }>();

  let activeSection = $state<'details' | 'diagnostics'>('details');

  $effect(() => {
    if (connection) activeSection = 'details';
  });

  function isNumber(value: unknown): value is number {
    return typeof value === 'number' && Number.isFinite(value);
  }

  function hasText(value: unknown): value is string {
    return typeof value === 'string' && value.trim().length > 0 && value !== '-';
  }

  function formatBytes(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  function formatRate(value: unknown): string {
    return isNumber(value) ? `${formatBytes(value)}/s` : '当前数据源未提供';
  }

  function formatDuration(startedAt?: number, durationMs?: number): string {
    if (isNumber(durationMs)) return formatElapsed(durationMs);
    if (isNumber(startedAt)) return formatElapsed(Math.max(0, Date.now() - startedAt));
    return '未提供';
  }

  function formatElapsed(elapsed: number): string {
    const seconds = Math.floor(elapsed / 1_000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }

  function formatTimestamp(timestamp: unknown): string {
    if (!isNumber(timestamp)) return '未提供';
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return '未提供';
    const formatted = date.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
    return `${formatted}.${String(date.getMilliseconds()).padStart(3, '0')}`;
  }

  function formatRaw(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function sourceLabel(item: DisplayConnection): string {
    if (hasText(item.source)) return item.source;
    if (hasText(item.processName)) return item.processName;
    if (hasText(item.inboundTag)) return `入口 ${item.inboundTag}`;
    return '内核查询未提供来源';
  }

  function modeLabel(mode?: string): string {
    switch (mode) {
      case 'global': return '全局';
      case 'rule': return '规则';
      case 'direct': return '直连';
      default: return mode ?? '未提供';
    }
  }

  function rawSourceLabel(source?: string): string {
    switch (source) {
      case 'event': return '事件流';
      case 'active_flows': return '活动连接查询';
      case 'recent_flows': return '连接记录查询';
      default: return '未关联原始记录';
    }
  }
</script>

{#if connection}
  <div
    class="drawer-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.currentTarget === event.target) onclose();
    }}
  >
    <aside class="drawer" role="dialog" aria-modal="true" aria-label="连接详情">
      <header class="drawer-header">
        <div class="drawer-heading">
          <div class="drawer-title-row">
            <span class="drawer-title" title={connection.destination}>{connection.destination}</span>
            <span class="tag">{connection.protocol.toUpperCase()}</span>
            <span class:active-state={connection.origin === 'active'} class="state-tag">
              {connection.origin === 'active' ? '活动中' : '已结束'}
            </span>
          </div>
          <div class="drawer-subtitle">
            <span>{sourceLabel(connection)}</span>
            {#if hasText(connection.outboundTag)}
              <span class="route-arrow">→</span>
              <span>{connection.outboundTag}</span>
            {/if}
            <span class="flow-id">#{connection.flowId}</span>
          </div>
        </div>
        <button class="icon-button" type="button" onclick={onclose} aria-label="关闭连接详情" title="关闭详情">
          <X size={17} />
        </button>
      </header>

      <nav class="drawer-tabs" aria-label="连接详情栏目">
        <button type="button" class:active={activeSection === 'details'} onclick={() => activeSection = 'details'}>详情</button>
        <button type="button" class:active={activeSection === 'diagnostics'} onclick={() => activeSection = 'diagnostics'}>诊断数据</button>
      </nav>

      <div class="drawer-content">
        {#if activeSection === 'details'}
          <section class="summary-grid">
            <article class="summary-card">
              <span class="summary-label">上传</span>
              <strong>{formatBytes(connection.bytesUp)}</strong>
              {#if connection.origin === 'active'}<small>{formatRate(connection.throughputUpBps)}</small>{/if}
            </article>
            <article class="summary-card">
              <span class="summary-label">下载</span>
              <strong>{formatBytes(connection.bytesDown)}</strong>
              {#if connection.origin === 'active'}<small>{formatRate(connection.throughputDownBps)}</small>{/if}
            </article>
            <article class="summary-card">
              <span class="summary-label">持续时间</span>
              <strong>{formatDuration(connection.startedAtUnixMs, connection.durationMs)}</strong>
              <small>{connection.origin === 'active' ? '持续更新' : '最终时长'}</small>
            </article>
          </section>

          <section class="detail-section">
            <h3>连接信息</h3>
            <div class="property-grid">
              <div class="property"><span>来源</span><strong title={connection.source}>{sourceLabel(connection)}</strong></div>
              {#if hasText(connection.processName) || hasText(connection.processPath) || isNumber(connection.processId)}
                <div class="property"><span>进程</span><strong title={connection.processPath}>{connection.processName ?? connection.processPath ?? `PID ${connection.processId}`}</strong></div>
              {/if}
              <div class="property"><span>目标</span><strong title={connection.targetHost}>{connection.targetHost ?? connection.destination}</strong></div>
              {#if hasText(connection.targetIp)}<div class="property"><span>解析地址</span><strong>{connection.targetIp}</strong></div>{/if}
              {#if hasText(connection.sniffedHost)}<div class="property"><span>嗅探域名</span><strong>{connection.sniffedHost}</strong></div>{/if}
              {#if hasText(connection.remoteDestination)}<div class="property"><span>实际远端</span><strong>{connection.remoteDestination}</strong></div>{/if}
            </div>
          </section>

          <section class="detail-section">
            <h3>转发路径</h3>
            <div class="property-grid">
              {#if hasText(connection.inboundTag)}<div class="property"><span>入口</span><strong>{connection.inboundTag}{connection.inboundProtocol ? ` · ${connection.inboundProtocol}` : ''}</strong></div>{/if}
              {#if hasText(connection.outboundTag)}<div class="property"><span>出口</span><strong>{connection.outboundTag}{connection.outboundProtocol ? ` · ${connection.outboundProtocol}` : ''}</strong></div>{/if}
              {#if hasText(connection.policyTag)}<div class="property"><span>策略</span><strong>{connection.policyTag}</strong></div>{/if}
              {#if hasText(connection.routeMode)}<div class="property"><span>路由模式</span><strong>{modeLabel(connection.routeMode)}{connection.routeAction ? ` · ${connection.routeAction}` : ''}</strong></div>{/if}
              {#if hasText(connection.matchedRule)}<div class="property wide"><span>命中规则{isNumber(connection.matchedRuleIndex) ? ` #${connection.matchedRuleIndex}` : ''}</span><strong>{connection.matchedRule}</strong></div>{/if}
              {#if connection.selectionChain.length > 0}<div class="property wide"><span>选择链</span><strong>{connection.selectionChain.join(' → ')}</strong></div>{/if}
              {#if connection.relayChain.length > 0}<div class="property wide"><span>中继链</span><strong>{connection.relayChain.join(' → ')}</strong></div>{/if}
            </div>
          </section>

          <section class="detail-section">
            <h3>时间</h3>
            <div class="property-grid">
              <div class="property"><span>开始时间</span><strong>{formatTimestamp(connection.startedAtUnixMs)}</strong></div>
              <div class="property"><span>最后活动</span><strong>{formatTimestamp(connection.lastActivityAtUnixMs)}</strong></div>
              <div class="property"><span>速率采样时间</span><strong>{formatTimestamp(connection.updatedAtUnixMs)}</strong></div>
              {#if connection.origin === 'recent'}<div class="property"><span>结束时间</span><strong>{formatTimestamp(connection.endedAtUnixMs)}</strong></div>{/if}
            </div>
            <p class="section-note">“速率采样时间”来自内核吞吐统计的 sampled_at_unix_ms，并不是通用的数据修改时间。</p>
          </section>

          {#if connection.outcome || connection.failureMessage || connection.closeReason}
            <section class="detail-section">
              <h3>结果</h3>
              <div class="property-grid">
                {#if connection.outcome}<div class="property"><span>结果</span><strong>{connection.outcome}</strong></div>{/if}
                {#if connection.closeReason}<div class="property"><span>结束原因</span><strong>{connection.closeReason}</strong></div>{/if}
                {#if connection.failureMessage}<div class="property wide failure"><span>失败{connection.failureStage ? ` · ${connection.failureStage}` : ''}{connection.failureCode ? ` · ${connection.failureCode}` : ''}</span><strong>{connection.failureMessage}</strong></div>{/if}
              </div>
            </section>
          {/if}
        {:else}
          <section class="detail-section diagnostics-intro">
            <h3>事件与记录</h3>
            <p>这里保留内核原始结构，用于排查字段缺失和版本兼容问题；日常查看连接不需要展开原始 JSON。</p>
            <div class="property-grid">
              <div class="property"><span>数据来源</span><strong>{rawSourceLabel(connection.rawSource)}</strong></div>
              <div class="property"><span>记录版本</span><strong>{isNumber(connection.revision) ? connection.revision : '当前查询模型未提供'}</strong></div>
              {#if connection.eventType}<div class="property"><span>事件类型</span><strong>{connection.eventType}</strong></div>{/if}
              {#if isNumber(connection.eventSequence)}<div class="property"><span>事件序号</span><strong>{connection.eventSequence}</strong></div>{/if}
              {#if connection.eventId}<div class="property wide"><span>事件 ID</span><strong>{connection.eventId}</strong></div>{/if}
              {#if isNumber(connection.eventOccurredAtUnixMs)}<div class="property"><span>事件发生时间</span><strong>{formatTimestamp(connection.eventOccurredAtUnixMs)}</strong></div>{/if}
              {#if isNumber(connection.capturedAtUnixMs)}<div class="property"><span>客户端接收时间</span><strong>{formatTimestamp(connection.capturedAtUnixMs)}</strong></div>{/if}
            </div>
          </section>

          {#if connection.rawPayload !== undefined}
            <details class="raw-block" open><summary>内核原始记录</summary><pre>{formatRaw(connection.rawPayload)}</pre></details>
          {:else}
            <div class="raw-empty">没有关联到对应生命周期的原始内核记录。</div>
          {/if}
          {#if connection.rawEnvelope !== undefined}
            <details class="raw-block"><summary>完整事件报文</summary><pre>{formatRaw(connection.rawEnvelope)}</pre></details>
          {/if}
        {/if}
      </div>

      {#if connection.origin === 'active' && canTerminate}
        <footer class="drawer-footer">
          <div><strong>终止活动连接</strong><span>将请求内核立即取消该连接，应用可能会自动重新建立。</span></div>
          <button class="terminate-button" type="button" disabled={terminating} onclick={() => onrequestterminate?.(connection)}>
            <AlertTriangle size={15} />{terminating ? '终止中...' : '终止连接'}
          </button>
        </footer>
      {/if}
    </aside>
  </div>
{/if}

<style>
  .drawer-backdrop { position: absolute; inset: 0; z-index: 50; display: flex; justify-content: flex-end; background: rgb(0 0 0 / 0.28); backdrop-filter: blur(1px); }
  .drawer { width: min(560px, 92%); height: 100%; display: flex; flex-direction: column; background: var(--background); border-left: 1px solid var(--border); box-shadow: -16px 0 40px rgb(0 0 0 / 0.18); animation: drawer-in 0.18s ease-out; }
  @keyframes drawer-in { from { transform: translateX(18px); opacity: 0; } to { transform: translateX(0); opacity: 1; } }
  .drawer-header { display: flex; align-items: flex-start; gap: 12px; padding: 16px 18px 13px; border-bottom: 1px solid var(--border); }
  .drawer-heading { min-width: 0; flex: 1; }
  .drawer-title-row, .drawer-subtitle { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .drawer-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 14px; font-weight: 700; }
  .drawer-subtitle { margin-top: 5px; color: var(--muted-foreground); font-family: var(--font-mono); font-size: 11px; }
  .drawer-subtitle span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .flow-id { margin-left: auto; opacity: 0.55; }
  .route-arrow { opacity: 0.45; }
  .tag, .state-tag { flex-shrink: 0; border-radius: 5px; padding: 2px 6px; background: var(--muted); color: var(--muted-foreground); font-size: 10px; font-weight: 700; }
  .state-tag.active-state { background: color-mix(in srgb, var(--primary) 12%, transparent); color: var(--primary); }
  .icon-button { width: 30px; height: 30px; display: inline-flex; align-items: center; justify-content: center; border: 0; border-radius: 7px; background: transparent; color: var(--muted-foreground); cursor: pointer; }
  .icon-button:hover { background: var(--muted); color: var(--foreground); }
  .drawer-tabs { display: flex; gap: 4px; padding: 8px 18px 0; border-bottom: 1px solid var(--border); }
  .drawer-tabs button { border: 0; border-bottom: 2px solid transparent; padding: 8px 10px 9px; background: transparent; color: var(--muted-foreground); font-size: 12px; font-weight: 600; cursor: pointer; }
  .drawer-tabs button.active { border-bottom-color: var(--primary); color: var(--foreground); }
  .drawer-content { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 18px 22px; }
  .summary-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }
  .summary-card { min-width: 0; display: flex; flex-direction: column; gap: 4px; padding: 11px; border: 1px solid var(--border); border-radius: 9px; background: color-mix(in srgb, var(--muted) 45%, transparent); }
  .summary-label, .summary-card small { color: var(--muted-foreground); font-size: 10.5px; }
  .summary-card strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 13px; }
  .detail-section { margin-top: 18px; }
  .detail-section h3 { margin: 0 0 9px; font-size: 11px; font-weight: 700; color: var(--muted-foreground); text-transform: uppercase; letter-spacing: 0.05em; }
  .property-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 9px 18px; }
  .property { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .property.wide { grid-column: 1 / -1; }
  .property span { color: var(--muted-foreground); font-size: 10.5px; }
  .property strong { overflow-wrap: anywhere; font-family: var(--font-mono); font-size: 11.5px; font-weight: 500; }
  .property.failure strong, .property.failure span { color: var(--destructive); }
  .section-note, .diagnostics-intro p { margin: 9px 0 0; color: var(--muted-foreground); font-size: 10.5px; line-height: 1.55; }
  .raw-block { margin-top: 12px; overflow: hidden; border: 1px solid var(--border); border-radius: 8px; background: color-mix(in srgb, var(--muted) 40%, transparent); }
  .raw-block summary { padding: 9px 11px; cursor: pointer; color: var(--muted-foreground); font-size: 11px; font-weight: 600; }
  .raw-block pre { max-height: 420px; overflow: auto; margin: 0; padding: 11px; border-top: 1px solid var(--border); font-family: var(--font-mono); font-size: 10.5px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
  .raw-empty { margin-top: 12px; padding: 14px; border: 1px dashed var(--border); border-radius: 8px; color: var(--muted-foreground); font-size: 11px; text-align: center; }
  .drawer-footer { display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 12px 18px; border-top: 1px solid var(--border); background: var(--background); }
  .drawer-footer > div { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .drawer-footer strong { font-size: 11.5px; }
  .drawer-footer span { color: var(--muted-foreground); font-size: 10px; }
  .terminate-button { flex-shrink: 0; display: inline-flex; align-items: center; gap: 6px; border: 1px solid color-mix(in srgb, var(--destructive) 35%, var(--border)); border-radius: 7px; padding: 7px 10px; background: color-mix(in srgb, var(--destructive) 8%, transparent); color: var(--destructive); font-size: 11px; font-weight: 700; cursor: pointer; }
  .terminate-button:hover:not(:disabled) { background: color-mix(in srgb, var(--destructive) 14%, transparent); }
  .terminate-button:disabled { opacity: 0.55; cursor: not-allowed; }
  @media (max-width: 720px) { .drawer { width: 100%; } .summary-grid { grid-template-columns: 1fr; } .property-grid { grid-template-columns: 1fr; } }
</style>
