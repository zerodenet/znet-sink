<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Choice } from '$lib/components/ui/choice';
  import * as Dialog from '$lib/components/ui/dialog';
  import type { ProxyConfigProfile } from '$lib/types/domain';
  import { getAppErrorMessage } from '$lib/services/core';
  import { exportNativeProfile, documentReceiptMessage, type DocumentDelivery } from '$lib/services/native-documents';
  let { profile, disabled = false }: { profile: ProxyConfigProfile; disabled?: boolean } = $props();
  let target = $state<ProxyConfigProfile | null>(null);
  let busy = $state(false);
  let message = $state<string | null>(null);
  let confirmed = $state(false);
  function preview() { target = { ...profile }; confirmed = false; message = null; }
  async function deliver(delivery: DocumentDelivery) {
    if (!target || busy || !confirmed) return;
    const original = target;
    busy = true;
    try {
      const receipt = await exportNativeProfile(original.id, original.updatedAtUnixMs, delivery, confirmed);
      message = documentReceiptMessage(receipt);
    } catch (error) { message = getAppErrorMessage(error, '配置导出失败，原配置保留'); }
    finally { busy = false; }
  }
</script>
<Button variant="outline" size="sm" disabled={disabled || busy || Boolean(profile.managedSource)} onclick={preview}>导出</Button>
<Dialog.Root open={target !== null} onOpenChange={(open) => { if (!open && !busy) target = null; }}>
  <Dialog.Content>
    <Dialog.Header><Dialog.Title>导出普通配置</Dialog.Title><Dialog.Description>导出“{target?.name}”的当前本地正文，可能包含服务器、账号和代理凭据。系统保存或分享由你选择；托管配置不能通过此入口导出。</Dialog.Description></Dialog.Header>
    <label class="acknowledgement"><Choice bind:checked={confirmed} disabled={busy} />我理解文件可能包含代理凭据，确认导出这份配置</label>
    {#if message}<p role="status">{message}</p>{/if}
    <Dialog.Footer>
      <Button variant="outline" disabled={busy} onclick={() => target = null}>返回</Button>
      <Button variant="outline" disabled={busy || !confirmed} onclick={() => deliver('share')}>系统分享</Button>
      <Button disabled={busy || !confirmed} onclick={() => deliver('save')}>{busy ? '等待系统结果…' : '保存到文件'}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>.acknowledgement { display:flex; align-items:center; gap:12px; min-height:44px; line-height:1.6; } p { overflow-wrap:anywhere; }</style>
