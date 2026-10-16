<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import { previewNativeDiagnostics, type DiagnosticPreview } from '$lib/services/native-diagnostics';
  import { guiExportDiagnostics, guiDiscardDiagnostics, type GuiDiagnosticExport } from '$lib/services/core';

  interface Gateway {
    preview(): Promise<DiagnosticPreview>;
    deliver(planId: string): Promise<GuiDiagnosticExport>;
    discard(planId: string): Promise<void>;
  }
  let {
    disabled = false,
    gateway = { preview: previewNativeDiagnostics, deliver: guiExportDiagnostics, discard: guiDiscardDiagnostics },
    onexported,
    onbusy,
  }: {
    disabled?: boolean;
    gateway?: Gateway;
    onexported?: (receipt: GuiDiagnosticExport) => void | Promise<void>;
    onbusy?: (busy: boolean) => void;
  } = $props();
  let open = $state(false);
  let preparing = $state(false);
  let busy = $state(false);
  let preview = $state.raw<DiagnosticPreview | null>(null);
  let message = $state<string | null>(null);
  let generation = 0;
  let alive = true;
  function discard(fixed: DiagnosticPreview | null) {
    if (fixed) void gateway.discard(fixed.planId).catch(() => {});
  }
  onDestroy(() => { alive = false; generation++; discard(preview); });
  function close() {
    if (busy) return;
    generation++;
    open = false;
    preparing = false;
    discard(preview);
    preview = null;
  }
  async function prepare() {
    if (disabled || open || preparing || busy) return;
    const current = ++generation;
    preview = null;
    message = null;
    preparing = true;
    open = true;
    try {
      const result = await gateway.preview();
      if (alive && current === generation) preview = result;
      else discard(result);
    } catch {
      if (alive && current === generation) message = '诊断快照准备失败，未导出文件；不会改用原始日志。';
    } finally {
      if (alive && current === generation) preparing = false;
    }
  }
  async function deliver() {
    const fixed = preview;
    if (disabled || busy || !fixed) return;
    busy = true;
    onbusy?.(true);
    // Retire before dispatch: errors/unknown IO never reuse the same plan.
    preview = null;
    try {
      const receipt = await gateway.deliver(fixed.planId);
      if (!alive) return;
      message = `诊断材料已保存（${receipt.files.length} 个文件）；仅包含已预览的公共投影。`;
      try { await onexported?.(receipt); } catch {
        if (alive) message = '诊断材料已保存；目录打开或列表刷新未确认，请检查已生成的诊断目录。';
      }
    } catch (error) {
      const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
      if (alive) message = ['conflict', 'invalid_argument', 'client_core_error'].includes(String(code))
        ? '原诊断预览已失效或请求被拒绝，未发起新的文件写入。请关闭后重新预览；不会重放原计划。'
        : '诊断导出未确认，请检查本地诊断目录；不会重试原计划。需要再次导出时请关闭后重新预览。';
    } finally {
      if (alive) busy = false;
      onbusy?.(false);
    }
  }
</script>

<Button variant="outline" size="sm" disabled={disabled || preparing || busy} onclick={prepare}>导出诊断材料</Button>
<DraggableModal title="预览桌面诊断材料" description="确认只保存本次固定的公共诊断快照。" {open} onClose={close} closeDisabled={busy} width="min(560px, 92vw)">
  <div class="diagnostic-preview">
    <p>只包含版本、客户端状态、已分类公共日志事实、有界 IPC 计量及任务元数据。本地日志及 IPC 是跨配置、实例的有界历史，任务限当前作用域。原始自由日志正文、未知字段、错误正文、原始文件、配置、URL、凭据及插件秘密均省略；原始本地记录保持不变。</p>
    {#if preparing}<p role="status">正在准备实际快照…</p>{/if}
    {#if preview}
      <article aria-label="固定桌面诊断快照">
        <p>日志 {preview.logCount} 条、有效已存 IPC 帧 {preview.frameCount ?? '—（读取失败）'} 条、任务 {preview.taskCount} 条；{preview.bytes} 字节（最高2MiB）。</p>
        <p>配置：{preview.scope.profileId ?? '无活动配置'} · 实例 {preview.scope.clientInstanceId} · revision {preview.scope.configRevision}</p>
        <p>五分钟内有效；配置或实例变化后必须重新预览。保存只使用此快照，不包含随后新增记录。</p>
        <details><summary>省略范围与原因</summary>{#each preview.omissions as omission}<p>{omission.category}：{omission.reason}{omission.count != null ? `（${omission.count}项）` : ''}</p>{/each}</details>
      </article>
    {/if}
    {#if message}<p role="status">{message}</p>{/if}
    <div class="actions"><Button variant="outline" disabled={busy} onclick={close}>返回</Button><Button disabled={disabled || busy || !preview} onclick={deliver}>{busy ? '等待保存回执…' : '确认保存原快照'}</Button></div>
  </div>
</DraggableModal>
<style>
  .diagnostic-preview {display:grid;gap:12px;min-width:0;} p {font-size:.85rem;line-height:1.6;overflow-wrap:anywhere;} .actions {display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px;} summary {cursor:pointer;min-height:44px;padding:10px 0;} .actions :global(button) {min-height:44px;}
</style>
