<script lang="ts">
  import type { ConnectionHistorySummary } from '$lib/types/debug';
  let { summary }: { summary?: ConnectionHistorySummary } = $props();
  function time(value: number | null) {
    return value === null ? '暂无' : new Date(value).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<div class="space-y-3 text-xs text-muted-foreground" role="note">
  {#if summary}
    <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2">
      <dt>本地记录</dt><dd>{summary.retainedRecords.toLocaleString()} 条 · {(summary.retainedBytes / 1024 / 1024).toFixed(2)} MiB</dd>
      <dt>符合筛选</dt><dd>{summary.matchedRecords.toLocaleString()} 条</dd>
      <dt>保留上限</dt><dd>{summary.recordLimit.toLocaleString()} 条 / {Math.round(summary.byteLimit / 1024 / 1024)} MiB / {Math.round(summary.maxAgeMs / 86400000)} 天</dd>
      <dt>最早接收</dt><dd>{time(summary.oldestCapturedAtMs)}</dd>
      <dt>最近接收</dt><dd>{time(summary.newestCapturedAtMs)}</dd>
      <dt>本次运行</dt><dd>已清理 {summary.removedSinceClientStart} 条 · 写入失败 {summary.writeFailuresSinceClientStart} 次</dd>
    </dl>
  {:else}<p>暂未读取到保留范围，请刷新记录后重试。</p>{/if}
  <p>仅保存已结束连接的信息，不含数据包或 HTTP 正文。离线期间可能缺失记录，缺失数量未知；上述时间不代表连续完整记录。</p>
  <p>按客户端接收时间筛选；暂停查看不停止后台记录。</p>
</div>
