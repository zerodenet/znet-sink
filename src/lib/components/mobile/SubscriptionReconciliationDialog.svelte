<script lang="ts">
  import { onMount } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import {
    reconcileUnknownSubscription,
    recoverConfirmedSubscriptionLocal,
    type SubscriptionReconciliation,
  } from '$lib/services/config';
  import type { SubscriptionProfile } from '$lib/types/domain';
  let {
    target,
    onClose,
    onRecovered,
  }: {
    target: SubscriptionProfile;
    onClose: () => void;
    onRecovered?: () => void | Promise<void>;
  } = $props();
  let checking = $state(false),
    result = $state<SubscriptionReconciliation | null>(null),
    failure = $state<string | null>(null);
  let saving = $state(false),
    attempted = $state(false),
    saved = $state(false);
  let alive = true,
    generation = 0;
  onMount(() => () => {
    alive = false;
    generation++;
  });
  $effect(() => {
    target.id;
    target.updatedAtUnixMs;
    target.targetProxyConfigId;
    generation++;
    checking = false;
    saving = false;
    attempted = false;
    saved = false;
    result = null;
    failure = null;
  });
  const materialLabels: Record<
    SubscriptionReconciliation['material']['status'],
    string
  > = {
    'saved-valid': '已有材料通过本地原生 schema 校验',
    'saved-invalid': '已有材料未通过本地校验',
    missing: '关联配置材料缺失',
    'not-linked': '未关联配置',
    'ownership-mismatch': '关联材料归属不匹配',
  };
  async function inspect() {
    if (checking || saving || saved) return;
    const current = ++generation;
    checking = true;
    result = null;
    failure = null;
    attempted = false;
    try {
      const next = await reconcileUnknownSubscription(
        target.id,
        target.updatedAtUnixMs,
        target.targetProxyConfigId ?? null,
      );
      if (alive && generation === current) result = next;
    } catch {
      if (alive && generation === current)
        failure =
          '核对未完成，来源或运行身份可能已变化。请刷新后重新核对；原结果仍未确认。';
    } finally {
      if (alive && generation === current) checking = false;
    }
  }
  async function recover() {
    if (
      checking ||
      saving ||
      attempted ||
      saved ||
      !result?.canResolve ||
      !result.recoveryId
    )
      return;
    const fixed = {
      id: result.source.id,
      version: result.source.updatedAtUnixMs,
      recoveryId: result.recoveryId,
    };
    const current = ++generation;
    saving = true;
    attempted = true;
    failure = null;
    try {
      await recoverConfirmedSubscriptionLocal(
        fixed.id,
        fixed.version,
        fixed.recoveryId,
      );
      if (!alive || generation !== current) return;
      saved = true;
      try {
        await onRecovered?.();
      } catch {
        if (alive && generation === current)
          failure = '本地已保存；列表刷新未完成，请返回后刷新。';
      }
    } catch {
      if (alive && generation === current)
        failure =
          '本地保存未确认，原核对记录保留。不会重新下载或提交运行配置；请重新核对后再决定是否保存。';
    } finally {
      if (alive && generation === current) saving = false;
    }
  }
</script>

<Dialog.Root
  open
  onOpenChange={(open) => {
    if (!open && !saving) onClose();
  }}
>
  <Dialog.Content
    class="reconciliation-dialog"
    showCloseButton={!saving}
    onEscapeKeydown={(event) => {
      if (saving) event.preventDefault();
    }}
    onInteractOutside={(event) => {
      if (saving) event.preventDefault();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>核对订阅的未知结果</Dialog.Title>
      <Dialog.Description
        >固定核对 {target.name} 的本地材料与当前运行身份；读取不下载、不应用，也不清除未知结果屏障。</Dialog.Description
      >
    </Dialog.Header>
    <div class="inspection-body">
      {#if checking}<p role="status">正在读取本地材料与运行身份…</p>{/if}
      {#if saving}<p role="status">正在完成原候选的本地保存…</p>{/if}
      {#if saved}<p role="status">
          本地已保存。此次恢复没有重新下载或提交运行配置。
        </p>{/if}
      {#if failure}<p role="alert">{failure}</p>{/if}
      {#if result}
        <dl>
          <dt>本地材料</dt>
          <dd>{materialLabels[result.material.status]}</dd>
          <dt>关联配置</dt>
          <dd>{result.material.profileId ?? '未关联'}</dd>
          <dt>本地选择</dt>
          <dd>
            {result.material.active === null
              ? '未提供'
              : result.material.active
                ? '本地已选择；运行生效未确认'
                : '本地未选择'}
          </dd>
          <dt>运行身份</dt>
          <dd>
            {result.runtime.status === 'readable'
              ? '当前身份可读；不代表原操作生效'
              : result.runtime.reason}
          </dd>
          <dt>内核实例</dt>
          <dd>{result.runtime.coreInstanceId ?? '—'}</dd>
          <dt>配置版本</dt>
          <dd>{result.runtime.configRevision ?? '—'}</dd>
          <dt>来源版本</dt>
          <dd>{result.source.updatedAtUnixMs}</dd>
          <dt>原候选材料</dt>
          <dd>
            {result.evidence.candidate === 'recorded'
              ? '已有持久记录'
              : '没有持久记录'}
          </dd>
          <dt>原应用回执</dt>
          <dd>
            {result.evidence.receipt === 'confirmed'
              ? '已有权威确认回执'
              : '没有持久记录'}
          </dd>
          {#if result.recoveryId}<dt>恢复记录</dt>
            <dd>{result.recoveryId}</dd>{/if}
        </dl>
        {#if result.material.validation === 'native-schema-only'}<p>
            本地 schema 校验不证明规则文件、编译协议或运行配置已生效。
          </p>{/if}
        {#if !saved}<p role="status">{result.reason}</p>{/if}
        {#if result.canResolve && !saved}<p>
            完成本地保存只发布这条记录中已确认的原候选材料。后端再次核对原来源版本、材料归属和运行回执；不会重新下载、重新应用或扩大目标范围。
          </p>{/if}
      {:else if !checking && !failure}
        <p>
          读取操作不会重试原提交。缺少持久候选或权威回执时，当前内核可读也不能解除未知状态。
        </p>
      {/if}
    </div>
    <Dialog.Footer>
      <Button variant="outline" disabled={saving} onclick={onClose}>返回</Button
      >
      {#if !saved}<Button
          variant="outline"
          disabled={checking || saving}
          onclick={inspect}
          >{result || failure ? '重新核对' : '读取核对状态'}</Button
        >{/if}
      {#if result?.canResolve && !saved}<Button
          disabled={checking || saving || attempted}
          onclick={recover}>完成本地保存</Button
        >{/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  :global(.reconciliation-dialog) {
    display: flex;
    flex-direction: column;
    max-height: calc(100dvh - 2rem);
  }
  .inspection-body {
    min-height: 0;
    overflow: auto;
    overflow-wrap: anywhere;
    font-size: 0.875rem;
    line-height: 1.6;
  }
  dl {
    display: grid;
    grid-template-columns: 5.5rem minmax(0, 1fr);
    gap: 0.65rem 0.75rem;
  }
  dt {
    color: var(--muted-foreground);
  }
  dd {
    margin: 0;
  }
  p {
    margin: 0.75rem 0;
  }
  :global(.reconciliation-dialog button) {
    min-height: 44px;
  }
</style>
