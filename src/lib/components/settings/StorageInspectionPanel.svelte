<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { Button } from '$lib/components/ui/button';
  import { inspectWorkspaceStorage, inspectPacketLedger } from '$lib/services/core';
  import { InspectionSession } from '$lib/features/storage-inspection/session';
  import { inspectionTime, scopeFingerprint, type InspectionKind, type WorkspaceStorageInspection, type PacketLedgerInspection } from '$lib/features/storage-inspection/model';
  let { kind, scopeKey = '' }: {kind: InspectionKind; scopeKey?: string} = $props();
  const session = new InspectionSession(untrack(() => kind), untrack(() => kind === 'workspace' ? inspectWorkspaceStorage : inspectPacketLedger), value => { view = value; });
  let view = $state(session.view);
  let priorScope = untrack(() => scopeKey);
  let observedHostScope: string|null = null;
  $effect(() => { if (scopeKey !== priorScope) { priorScope = scopeKey; session.invalidate(); } });
  onMount(() => {
    let alive = true, stop: (() => void)|undefined;
    void listen('client-core:updated', event => {
      let next: string|null;
      try { next = scopeFingerprint(event.payload); } catch { next = null; }
      if (next === null || next !== observedHostScope) session.invalidate();
      observedHostScope = next;
    }).then(unlisten => { if (alive) stop = unlisten; else unlisten(); }).catch(() => session.invalidate());
    return () => { alive = false; stop?.(); session.dispose(); };
  });
  const title = $derived(kind === 'workspace' ? '工作区存储只读检查' : 'Packet 账本只读检查');
  const yes = (value: boolean|null) => value === null ? '未确认' : value ? '是' : '否';
  const markerLabels = {'not-read':'未读取',absent:'无标记',empty:'空标记',initialized:'已初始化',untrusted:'无法校验'};
  const lockLabels = {'not-checked':'未检查','not-present':'无协调文件',acquired:'检查期间已取得',busy:'被占用',unsupported:'平台不支持',unavailable:'无法确认'};
</script>
<section class="storage-inspection" aria-label={title}>
  <div class="inspection-heading"><h3>{title}</h3><Button variant="outline" disabled={view.busy} onclick={() => session.inspect()}>查看本地恢复记录</Button></div>
  <p>仅检查本地存储元资料，不读取候选正文，不清账、不归档、不重放操作；原屏障保持。元资料可读不代表配置生效或会话结束。</p>
  {#if view.busy}<p role="status">正在只读检查本地存储…</p>{/if}
  {#if view.error}<p role="alert">{view.error}</p>{/if}
  {#if view.result}
    <div class="inspection-result" aria-label="本地存储检查结果">
      {#if view.historical}<p role="status">历史检查结果；作用域或存储状态可能已变化，请显式重新检查。</p>{/if}
      <p role="status">{view.result.summary}</p>
      <dl>
        <dt>检查时间</dt><dd>{inspectionTime(view.result.sampledAtUnixMs)}</dd>
        <dt>宿主身份</dt><dd>{view.result.hostSessionId ? '本次宿主提供；不授予控制权限' : '未确认；仅供只读参考'}</dd>
        <dt>存储采样稳定</dt><dd>{yes(view.result.evidence.snapshotStable)}</dd>
        {#if kind === 'workspace'}
          {@const result = view.result as WorkspaceStorageInspection}
          <dt>数据库存在</dt><dd>{yes(result.evidence.databasePresent)}</dd>
          <dt>数据库字节</dt><dd>{result.evidence.databaseBytes ?? '—'}</dd>
          <dt>应用标识</dt><dd>{result.evidence.applicationId ?? '—'}</dd>
          <dt>存储版本</dt><dd>{result.evidence.schemaVersion ?? '—'}</dd>
          <dt>元资料记录</dt><dd>{result.evidence.metadataRows ?? '—'}</dd>
          <dt>待处理记录</dt><dd>{result.evidence.pendingRows ?? '—'}</dd>
          <dt>候选正文</dt><dd>未读取</dd>
        {:else}
          {@const result = view.result as PacketLedgerInspection}
          <dt>账本存在</dt><dd>{yes(result.evidence.journalPresent)}</dd>
          <dt>账本字节</dt><dd>{result.evidence.journalBytes ?? '—'}</dd>
          <dt>初始化标记</dt><dd>{markerLabels[result.evidence.markerState]}</dd>
          <dt>文件协调</dt><dd>{lockLabels[result.evidence.lockState]}</dd>
          <dt>存储版本</dt><dd>{result.evidence.schemaVersion ?? '—'}</dd>
          <dt>已校验记录</dt><dd>{result.evidence.verifiedRecordCount ?? '—'}</dd>
          <dt>内存屏障数</dt><dd>{result.inMemoryBarrierCount ?? '—'}</dd>
          <dt>持久发布未确认</dt><dd>{yes(result.publicationUnconfirmed)}</dd>
          <dt>检查时新增关闭受阻</dt><dd>{yes(result.newCloseBlocked)}</dd>
          <dt>账本摘要</dt><dd>{result.evidence.journalSha256 ?? '—'}</dd>
        {/if}
      </dl>
      <p>此检查不授予恢复或关闭权限；继续使用原操作的独立核对流程。</p>
    </div>
  {:else if !view.busy && !view.error}<p>尚未检查。仅在点击后读取本地元资料。</p>{/if}
</section>
<style>
  .storage-inspection { min-width:0; display:grid; gap:10px; border:1px solid var(--border); border-radius:12px; padding:12px; }
  .inspection-heading {display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;}
  h3 {font-weight:600;font-size:14px;} p {font-size:12px;color:var(--muted-foreground);overflow-wrap:anywhere;}
  [role='alert'] {color:var(--destructive);}
  .inspection-result {display:grid;gap:10px;min-width:0;}
  dl {display:grid;grid-template-columns:minmax(70px,100px) minmax(0,1fr);gap:6px 10px;font-size:12px;}
  dt {color:var(--muted-foreground);} dd {min-width:0;overflow-wrap:anywhere;word-break:break-word;}
  .storage-inspection :global(button) {min-height:44px;white-space:normal;}
</style>
