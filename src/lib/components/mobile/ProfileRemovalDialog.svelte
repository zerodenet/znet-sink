<script lang="ts">
  import { untrack } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Choice } from '$lib/components/ui/choice';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import { getAppErrorMessage } from '$lib/services/core';
  import { profileRemovalGateway } from '$lib/features/profile-removal/client';
  import { RemovalSession } from '$lib/features/profile-removal/session';
  import type { RemovalGateway, RemovalResult, RemovalAction } from '$lib/features/profile-removal/types';
  let { targetId, open = $bindable(false), onremoved, gateway = profileRemovalGateway }: {
    targetId: string | null; open?: boolean; onremoved: (result: RemovalResult) => void | Promise<void>; gateway?: RemovalGateway;
  } = $props();
  let mode = $state<'replace' | 'stop' | ''>('');
  let replacementId = $state('');
  let confirmed = $state(false);
  const session = new RemovalSession(untrack(() => gateway), next => { view = next; }, result => { void onremoved(result); });
  let view = $state(session.view);
  const options = $derived((view.preview?.replacements ?? []).map(r => ({ value: r.id, label: r.reason ? `${r.name}（${r.reason}）` : r.name, disabled: !!r.reason })));
  const action = $derived<RemovalAction | null>(view.preview && !view.preview.active ? { kind: 'inactive' } : mode === 'replace' && replacementId ? { kind: 'replace', replacementId } : mode === 'stop' ? { kind: 'stop' } : null);
  $effect(() => {
    const id = targetId;
    const opened = open;
    untrack(() => {
      mode = ''; replacementId = ''; confirmed = false;
      if (opened && id) void session.open(id); else session.close();
    });
  });
</script>
<Dialog.Root bind:open>
  <Dialog.Content class="profile-removal-dialog" showCloseButton={!view.busy} onInteractOutside={(event) => { if (view.busy) event.preventDefault(); }} onEscapeKeydown={(event) => { if (view.busy) event.preventDefault(); }}>
    <Dialog.Header><Dialog.Title>删除配置</Dialog.Title><Dialog.Description>先核对对象、来源关系和当前运行状态，再提交固定目标。不会自动选第一份配置作为替代。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      {#if view.loading}<p role="status">正在读取删除预检…</p>{/if}
      {#if view.error}<p class="notice" role="alert">{getAppErrorMessage(view.error, '删除预检或提交失败')}。原操作需要核对时，请勿重复提交。</p>{/if}
      {#if view.result}<p class="notice" role="status">{view.result.message}</p>{/if}
      {#if view.preview}
        <dl><dt>配置</dt><dd>{view.preview.targetName}</dd><dt>对象 ID</dt><dd>{view.preview.targetId}</dd><dt>来源</dt><dd>{view.preview.sourceType === 'managed' ? '插件托管' : view.preview.sourceType === 'subscription' ? '订阅关联配置' : '普通本地配置'}</dd><dt>版本</dt><dd>{view.preview.targetVersion}</dd><dt>使用中</dt><dd>{view.preview.active ? '是' : '否'}</dd><dt>运行态</dt><dd>{view.preview.runtime.state === 'running' ? '内核运行已确认' : view.preview.runtime.state === 'offline' ? '本地内核离线' : '平台或运行状态未确认'}</dd><dt>本地覆盖</dt><dd>{view.preview.overrideCount} 项，随配置清理</dd></dl>
        {#if view.preview.subscriptions.length}<p>以下订阅将保留，解除它们与此配置的关联；来源地址、同步材料及权限不会被删除：</p><ul>{#each view.preview.subscriptions as relation (relation.id)}<li>{relation.name} · {relation.id}</li>{/each}</ul>{:else}<p>没有关联订阅。</p>{/if}
        {#if view.preview.blockedReason}<p class="notice" role="status">{view.preview.blockedReason}</p>{/if}
        {#if view.preview.active && !view.preview.blockedReason}
          <fieldset disabled={view.busy || view.unconfirmed}><legend>删除使用中的配置后</legend>
            <label class="choice"><Choice type="radio" name="profile-removal-action" checked={mode === 'replace'} onchange={() => { mode = 'replace'; confirmed = false; }} />明确选择替代配置</label>
            <FieldSelect aria-label="替代配置" value={replacementId} {options} placeholder="请选择替代配置" disabled={mode !== 'replace' || view.busy} onValueChange={(value) => { replacementId = value; confirmed = false; }} />
            <label class="choice"><Choice type="radio" name="profile-removal-action" checked={mode === 'stop'} onchange={() => { mode = 'stop'; confirmed = false; }} />停止运行并删除，不自动启用其他配置</label>
          </fieldset>
          <p class="notice">{mode === 'stop' ? '先实际停止运行并释放本客户端接管；停止未确认时保留配置和关系。' : '替代正文和来源版本再次校验；运行应用结果未确认时保留原配置。离线替代仅保存为待应用。'}</p>
        {/if}
        {#if !view.preview.blockedReason && !view.unconfirmed}<label class="choice"><Choice bind:checked={confirmed} disabled={view.busy || !action} />我确认删除上述固定配置，并接受所列关联及连接影响</label>{/if}
      {/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={view.busy} onclick={() => { open = false; }}>取消</Button>
      {#if view.unconfirmed}<Button variant="outline" disabled={view.busy} onclick={() => session.reconcile()}>核对原操作</Button>
      {:else}<Button variant="outline" disabled={view.busy || view.loading || !targetId} onclick={() => { mode = ''; replacementId = ''; confirmed = false; if (targetId) void session.open(targetId); }}>重新预览</Button><Button variant="destructive" disabled={view.busy || view.loading || !action || !confirmed || !!view.preview?.blockedReason} onclick={() => { if (action) void session.commit(action); }}>{view.busy ? '等待确认…' : '确认删除'}</Button>{/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  dl { display: grid; grid-template-columns: 76px minmax(0,1fr); gap: 7px 12px; margin: 0 0 14px; } dt { color: var(--muted-foreground); } dd,li { overflow-wrap: anywhere; word-break: break-word; }
  .notice { background: var(--muted); padding: 12px; border-radius: 9px; margin: 12px 0; }
  fieldset { min-width: 0; display: grid; gap: 10px; margin: 14px 0; } legend { font-weight: 600; margin-bottom: 8px; }
  .choice { display: flex; min-height: 44px; align-items: center; gap: 10px; padding: 4px 0; }
  ul { margin: 8px 0 14px; padding-left: 20px; }
  :global(.profile-removal-dialog button) { min-height: 44px; }
</style>
