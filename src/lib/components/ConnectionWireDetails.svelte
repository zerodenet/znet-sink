<script lang="ts">
  import type { DisplayConnection } from '$lib/services/connection-view';

  let { connection } = $props<{ connection: DisplayConnection }>();

  const occurredAt = $derived(
    connection.eventOccurredAtUnixMs
      ?? connection.endedAtUnixMs
      ?? connection.startedAtUnixMs
      ?? connection.updatedAtUnixMs,
  );

  function formatTimestamp(timestamp?: number): string {
    if (timestamp === undefined) return '-';
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return '-';
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

  function rawSourceLabel(source?: string): string {
    switch (source) {
      case 'event': return '事件流';
      case 'active_flows': return '活动连接查询';
      case 'recent_flows': return '连接记录查询';
      default: return '内核';
    }
  }
</script>

<div class="wire-section">
  <div class="wire-title">时间与事件</div>
  <div class="wire-grid">
    <div class="wire-item">
      <span class="wire-key">发生时间</span>
      <span class="wire-value">{formatTimestamp(occurredAt)}</span>
    </div>
    {#if connection.startedAtUnixMs !== undefined}
      <div class="wire-item">
        <span class="wire-key">开始时间</span>
        <span class="wire-value">{formatTimestamp(connection.startedAtUnixMs)}</span>
      </div>
    {/if}
    {#if connection.lastActivityAtUnixMs !== undefined}
      <div class="wire-item">
        <span class="wire-key">最后活动</span>
        <span class="wire-value">{formatTimestamp(connection.lastActivityAtUnixMs)}</span>
      </div>
    {/if}
    {#if connection.updatedAtUnixMs !== undefined}
      <div class="wire-item">
        <span class="wire-key">数据更新时间</span>
        <span class="wire-value">{formatTimestamp(connection.updatedAtUnixMs)}</span>
      </div>
    {/if}
    {#if connection.endedAtUnixMs !== undefined}
      <div class="wire-item">
        <span class="wire-key">结束时间</span>
        <span class="wire-value">{formatTimestamp(connection.endedAtUnixMs)}</span>
      </div>
    {/if}
    {#if connection.eventType}
      <div class="wire-item">
        <span class="wire-key">事件类型</span>
        <span class="wire-value">{connection.eventType}</span>
      </div>
    {/if}
    {#if connection.eventSequence !== undefined}
      <div class="wire-item">
        <span class="wire-key">事件序号</span>
        <span class="wire-value">{connection.eventSequence}</span>
      </div>
    {/if}
    {#if connection.eventId}
      <div class="wire-item wire-wide">
        <span class="wire-key">事件 ID</span>
        <span class="wire-value">{connection.eventId}</span>
      </div>
    {/if}
    {#if connection.capturedAtUnixMs !== undefined}
      <div class="wire-item">
        <span class="wire-key">客户端接收时间</span>
        <span class="wire-value">{formatTimestamp(connection.capturedAtUnixMs)}</span>
      </div>
    {/if}
  </div>

  {#if connection.rawPayload !== undefined}
    <details class="raw-block">
      <summary>
        <span>内核原始记录</span>
        <span class="raw-source">{rawSourceLabel(connection.rawSource)}</span>
      </summary>
      <pre>{formatRaw(connection.rawPayload)}</pre>
    </details>
  {/if}

  {#if connection.rawEnvelope !== undefined}
    <details class="raw-block">
      <summary>
        <span>事件原始报文</span>
        <span class="raw-source">完整事件信封</span>
      </summary>
      <pre>{formatRaw(connection.rawEnvelope)}</pre>
    </details>
  {/if}
</div>

<style>
  .wire-section {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
    padding-top: 9px;
    border-top: 1px solid var(--border);
  }

  .wire-title {
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted-foreground);
    opacity: 0.78;
  }

  .wire-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 6px 16px;
  }

  .wire-item {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .wire-wide {
    grid-column: 1 / -1;
  }

  .wire-key {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .wire-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--foreground);
  }

  .raw-block {
    border: 1px solid var(--border);
    border-radius: 7px;
    background: color-mix(in srgb, var(--background) 75%, transparent);
    overflow: hidden;
  }

  .raw-block summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 9px;
    cursor: pointer;
    font-size: 11px;
    font-weight: 600;
    color: var(--foreground);
    user-select: none;
  }

  .raw-source {
    font-size: 10px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .raw-block pre {
    max-height: 320px;
    overflow: auto;
    margin: 0;
    padding: 10px;
    border-top: 1px solid var(--border);
    background: color-mix(in srgb, var(--muted) 55%, var(--background));
    color: var(--foreground);
    font-family: var(--font-mono);
    font-size: 10.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
