<script lang="ts">
  import type { ConnectionHistorySummary } from '$lib/types/debug';
  let { summary }: { summary?: ConnectionHistorySummary } = $props();
  function time(value: number | null) {
    return value === null ? '暂无' : new Date(value).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<details class="border-b border-border px-3 py-2 text-[10.5px] text-muted-foreground">
  <summary class="cursor-pointer">记录范围与保留限制
    {#if summary} · 本地 {summary.retainedRecords} 条 · 筛选匹配 {summary.matchedRecords} 条{/if}
    {#if summary && summary.writeFailuresSinceClientStart > 0} · 记录写入异常{/if}
  </summary>
  <div class="mt-2 max-h-32 space-y-1 overflow-y-auto break-words" role="note">
    <p>仅保存客户端收到的已结束连接信息，不含数据包或 HTTP 正文。断连或客户端未运行期间可能缺失记录，缺失数量未知。</p>
    {#if summary}
      <p>最多保留 {summary.recordLimit.toLocaleString()} 条、{Math.round(summary.byteLimit / 1024 / 1024)} MiB、{Math.round(summary.maxAgeMs / 86400000)} 天，任一限制达到后清理旧记录。当前占用 {(summary.retainedBytes / 1024 / 1024).toFixed(2)} MiB。</p>
      <p>现存记录接收时间：{time(summary.oldestCapturedAtMs)} — {time(summary.newestCapturedAtMs)}。该范围不代表连续完整记录。</p>
      <p>本次客户端运行期间清理 {summary.removedSinceClientStart} 条过期、超限或无效记录；写入/维护失败 {summary.writeFailuresSinceClientStart} 次。计数不包含此前运行。</p>
    {:else}
      <p>保留范围暂未读取，请刷新记录；无法据此判断记录完整性。</p>
    {/if}
    <p>时间筛选使用客户端接收时间；暂停查看只暂停列表更新，后台仍会记录。</p>
  </div>
</details>
