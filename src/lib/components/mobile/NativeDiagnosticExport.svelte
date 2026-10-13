<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { getNativeDocumentCapabilities, exportNativeDiagnostics, documentReceiptMessage, type DocumentDelivery } from '$lib/services/native-documents';
  import { getAppErrorMessage } from '$lib/services/core';
  let { disabled = false }: { disabled?: boolean } = $props();
  let open = $state(false);
  let available = $state(false);
  let busy = $state(false);
  let message = $state<string | null>(null);
  onMount(() => {
    let alive = true;
    void getNativeDocumentCapabilities().then(value => { if (alive) available = value.available; })
      .catch(() => { if (alive) message = '系统保存与分享适配暂不可用'; });
    return () => { alive = false; };
  });
  async function deliver(delivery: DocumentDelivery) {
    if (busy || !available) return;
    busy = true;
    try { message = documentReceiptMessage(await exportNativeDiagnostics(delivery)); }
    catch (error) { message = getAppErrorMessage(error, '诊断文件导出失败，原数据保留'); }
    finally { busy = false; }
  }
</script>
<Button variant="outline" size="sm" disabled={disabled || busy} onclick={() => { open = true; message = null; }}>导出诊断材料</Button>
<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header><Dialog.Title>预览诊断文件范围</Dialog.Title><Dialog.Description>包含客户端版本、平台契约状态，以及最近最多 200 条日志的时间、级别和来源。当前原生导出不包含日志消息、结构字段和调试正文；代理配置、订阅 URL、凭据和插件秘密始终排除。系统分享由你主动发起。</Dialog.Description></Dialog.Header>
    {#if !available}<p role="status">系统文件与分享适配暂不可用，请稍后重试。</p>{/if}
    {#if message}<p role="status">{message}</p>{/if}
    <Dialog.Footer><Button variant="outline" disabled={busy} onclick={() => open = false}>返回</Button><Button variant="outline" disabled={!available || busy} onclick={() => deliver('share')}>系统分享</Button><Button disabled={!available || busy} onclick={() => deliver('save')}>{busy ? '等待系统结果…' : '保存到文件'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
