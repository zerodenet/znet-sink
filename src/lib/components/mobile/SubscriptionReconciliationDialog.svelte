<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { reconcileUnknownSubscription, type SubscriptionReconciliation } from '$lib/services/config';
  import type { SubscriptionProfile } from '$lib/types/domain';
  let { target, onClose }: { target: SubscriptionProfile; onClose: () => void } = $props();
  let checking = $state(false), result = $state<SubscriptionReconciliation | null>(null), failure = $state<string | null>(null);
  let alive = true, generation = 0;
  onMount(() => () => { alive = false; generation++; });
  $effect(() => { target.id; target.updatedAtUnixMs; target.targetProxyConfigId; generation++; checking = false; result = null; failure = null; });
  const materialLabels: Record<SubscriptionReconciliation['material']['status'], string> = {
    'saved-valid': '已有材料通过本地原生 schema 校验', 'saved-invalid': '已有材料未通过本地校验',
    missing: '关联配置材料缺失', 'not-linked': '未关联配置', 'ownership-mismatch': '关联材料归属不匹配',
  };
  async function inspect() {
    if (checking) return;
    const current = ++generation; checking = true; result = null; failure = null;
    try {
      const next = await reconcileUnknownSubscription(target.id, target.updatedAtUnixMs, target.targetProxyConfigId ?? null);
      if (alive && generation === current) result = next;
    } catch {
      if (alive && generation === current) failure = '核对未完成，来源或运行身份可能已变化。请刷新后重新核对；原结果仍未确认。';
    } finally { if (alive && generation === current) checking = false; }
  }
</script>
<Dialog.Root open onOpenChange={open => { if (!open) onClose(); }}>
  <Dialog.Content class="reconciliation-dialog">
    <Dialog.Header>
      <Dialog.Title>核对订阅的未知结果</Dialog.Title>
      <Dialog.Description>固定核对 {target.name} 的本地材料与当前运行身份；不下载、不应用，也不清除未知结果屏障。</Dialog.Description>
    </Dialog.Header>
    <div class="inspection-body">
      {#if checking}<p role="status">正在读取本地材料与运行身份…</p>{/if}
      {#if failure}<p role="alert">{failure}</p>{/if}
      {#if result}
        <dl>
          <dt>本地材料</dt><dd>{materialLabels[result.material.status]}</dd>
          <dt>关联配置</dt><dd>{result.material.profileId ?? '未关联'}</dd>
          <dt>本地选择</dt><dd>{result.material.active === null ? '未提供' : result.material.active ? '本地已选择；运行生效未确认' : '本地未选择'}</dd>
          <dt>运行身份</dt><dd>{result.runtime.status === 'readable' ? '当前身份可读；不代表原操作生效' : result.runtime.reason}</dd>
          <dt>内核实例</dt><dd>{result.runtime.coreInstanceId ?? '—'}</dd>
          <dt>配置版本</dt><dd>{result.runtime.configRevision ?? '—'}</dd>
          <dt>原候选材料</dt><dd>没有持久记录</dd>
          <dt>原应用回执</dt><dd>没有持久记录</dd>
        </dl>
        {#if result.material.validation === 'native-schema-only'}<p>本地 schema 校验不证明规则文件、编译协议或运行配置已生效。</p>{/if}
        <p role="status">{result.reason}</p>
      {:else if !checking && !failure}
        <p>读取操作不会重试原提交。缺少持久候选或权威回执时，当前内核可读也不能解除未知状态。</p>
      {/if}
    </div>
    <Dialog.Footer><Button variant="outline" onclick={onClose}>返回</Button><Button disabled={checking} onclick={inspect}>{result || failure ? '重新核对' : '读取核对状态'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  :global(.reconciliation-dialog) { display:flex; flex-direction:column; max-height:calc(100dvh - 2rem); }
  .inspection-body { min-height:0; overflow:auto; overflow-wrap:anywhere; font-size:.875rem; line-height:1.6; }
  dl { display:grid; grid-template-columns:5.5rem minmax(0,1fr); gap:.65rem .75rem; }
  dt { color:var(--muted-foreground); } dd { margin:0; } p { margin:.75rem 0; }
  :global(.reconciliation-dialog button) { min-height:44px; }
</style>
