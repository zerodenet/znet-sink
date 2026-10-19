<script lang="ts">
  import { upsertSubscription, syncSubscription } from '$lib/services/config';
  import { canonicalSubscriptionFormat } from '$lib/features/configuration/catalog';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { SubscriptionProfile } from '$lib/types/domain';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { Choice } from '$lib/components/ui/choice';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  let { source = null, onsaved, onclose }: {
    source?: SubscriptionProfile | null; onsaved: () => Promise<void>; onclose: () => void;
  } = $props();
  const formats = [{ value: 'auto', label: '自动检测' }, { value: 'zero', label: 'Zero' }, { value: 'clash', label: 'Clash' }];
  let name = $state(''); let url = $state(''); let format = $state('auto');
  let interval = $state<number | undefined>(undefined); let userAgent = $state(''); let enabled = $state(true);
  let saving = $state(false); let error = $state(''); let savedId = $state<string | null>(null);
  $effect(() => {
    name = source?.name ?? ''; url = source?.url ?? ''; format = canonicalSubscriptionFormat(source?.format ?? 'auto');
    interval = source?.updateIntervalSecs ?? 0; userAgent = source?.userAgent ?? ''; enabled = source?.enabled ?? true;
  });
  async function save() {
    if (saving || source?.managedSource || savedId) return;
    saving = true; error = '';
    try {
      if (!name.trim() || !url.trim()) throw new Error('请输入名称和订阅地址');
      if (interval != null && (!Number.isInteger(interval) || (interval !== 0 && interval < 60))) throw new Error('更新间隔为 0（手动）或至少 60 秒');
      const saved = await upsertSubscription({ id: source?.id, name: name.trim(), url: url.trim(), format,
        enabled, kernel: source?.kernel ?? 'zero', updateIntervalSecs: interval || undefined,
        userAgent: userAgent.trim() || undefined,
        // The source remains part of its original configuration; no relinking UI.
        targetProxyConfigId: source?.targetProxyConfigId });
      savedId = saved.id;
      if (!source) {
        try { await syncSubscription(saved.id); }
        catch (cause) {
          // Creation succeeded. Keep the pending source visible; never create a duplicate on retry.
          error = `订阅来源已保存，首次更新失败：${getAppErrorMessage(cause, '更新失败')}。可关闭此窗口，在配置列表重试。`;
          await onsaved(); return;
        }
      }
      await onsaved(); onclose();
    } catch (cause) { error = getAppErrorMessage(cause, '保存来源失败'); }
    finally { saving = false; }
  }
</script>
<Dialog.Root open onOpenChange={(value) => { if (!value && !saving) onclose(); }}>
  <Dialog.Content>
    <Dialog.Header><Dialog.Title>{source ? '编辑订阅来源' : '添加订阅配置'}</Dialog.Title><Dialog.Description>更新内容会保留此配置的策略选择和客户端全局设置。</Dialog.Description></Dialog.Header>
    <Dialog.Body class="grid gap-[15px]">
      <label class="grid gap-2">名称<Input bind:value={name} disabled={saving || !!savedId} /></label>
      <label class="grid gap-2">订阅地址<Input bind:value={url} type="url" disabled={saving || !!savedId} /></label>
      <FieldSelect bind:value={format} options={formats} disabled={saving || !!savedId} aria-label="订阅格式" />
      <label class="grid gap-2">自动更新间隔（秒，0 为手动）<Input type="number" min={0} step={1} bind:value={interval} disabled={saving || !!savedId} /></label>
      <label class="flex gap-2"><Choice bind:checked={enabled} disabled={saving || !!savedId} />启用此订阅来源（允许手动及自动更新）</label>
      <label class="grid gap-2">User-Agent<Input bind:value={userAgent} disabled={saving || !!savedId} aria-describedby="source-user-agent-help" /></label><p id="source-user-agent-help" class="text-muted-foreground text-xs">填写后完全覆盖默认 User-Agent</p>
      {#if source}<p class="text-muted-foreground text-xs">保存后，在配置列表点击更新以获取新内容。</p>{/if}
      {#if error}<p role="alert" class="text-destructive">{error}</p>{/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={saving} onclick={onclose}>{savedId ? '关闭' : '取消'}</Button><Button disabled={saving || !!savedId} onclick={save}>{saving ? '保存中…' : source ? '保存来源' : '保存并更新'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
