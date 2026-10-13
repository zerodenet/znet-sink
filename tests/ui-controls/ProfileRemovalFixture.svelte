<script lang="ts">
  import ProfileRemovalDialog from '$lib/components/mobile/ProfileRemovalDialog.svelte';
  import { Button } from '$lib/components/ui/button';
  import type { RemovalGateway, RemovalInput, RemovalPreview, RemovalResult } from '$lib/features/profile-removal/types';
  type Mode = 'replace' | 'stop' | 'unknown' | 'native-active' | 'native-inactive';
  const queryMode = new URLSearchParams(window.location.search).get('removal');
  let { mode = (['replace', 'stop', 'unknown', 'native-active', 'native-inactive'].includes(queryMode ?? '') ? queryMode : 'replace') as Mode }: { mode?: Mode } = $props();
  let open = $state(true);
  let calls = $state<{ kind: string; targetId?: string; input?: RemovalInput }[]>([]);
  let outcomes = $state<RemovalResult[]>([]);
  let uncertainPending = false;
  const targetId = $derived(mode === 'native-inactive' ? 'profile-inactive' : 'profile-current');
  function snapshot(): RemovalPreview {
    const native = mode === 'native-active' || mode === 'native-inactive';
    return {
      targetId, targetName: native && mode === 'native-inactive' ? '未使用的本地配置' : '正在使用的来源配置',
      targetVersion: '18446744073709551615', active: mode !== 'native-inactive', sourceType: 'subscription',
      guard: 'fixture-fixed-source-version-and-relationship-proof',
      subscriptions: [{ id: 'subscription-fixed', name: '保留的关联订阅' }],
      replacements: mode === 'stop' ? [] : [
        { id: 'replacement-first', name: '列表首项（不会自动选中）', reason: null },
        { id: 'replacement-chosen', name: '明确指定的替代配置', reason: null },
        { id: 'replacement-protected', name: '托管材料', reason: '须通过来源管理' },
      ],
      runtime: { state: native ? 'platform_unknown' : 'running', coreInstanceId: native ? null : 'core-fixture', configRevision: native ? null : '18446744073709551615', managedPid: native ? null : 42 },
      blockedReason: mode === 'native-active' ? '平台 VPN/运行服务状态尚未确认；不能删除使用中的配置' : uncertainPending ? '此前删除协调结果未确认；请核对原操作，不能重复提交' : null,
      pending: uncertainPending, overrideCount: 2,
    };
  }
  const gateway: RemovalGateway = {
    async preview(id) { calls = [...calls, { kind: 'preview', targetId: id }]; return snapshot(); },
    async commit(input) {
      // Test-only public gateway: the production dialog/session executes all
      // admission and fixed-target logic, while no host mutation is invoked.
      calls = [...calls, { kind: 'commit', input: structuredClone(input) }];
      await new Promise(resolve => setTimeout(resolve, 80));
      if (mode === 'unknown') {
        uncertainPending = true;
        return { status: 'unconfirmed', message: '测试回执丢失：删除结果未确认，仅能核对原操作', removedId: null, replacementId: null, runtimeStopped: false };
      }
      return { status: 'confirmed', message: '测试 gateway 已确认固定目标', removedId: input.targetId,
        replacementId: input.action.kind === 'replace' ? input.action.replacementId : null,
        runtimeStopped: input.action.kind === 'stop' };
    },
    async reconcile(id) {
      calls = [...calls, { kind: 'reconcile', targetId: id }];
      return { status: 'unconfirmed', message: '测试运行结果仍待核对；未重放删除或替代应用', removedId: null, replacementId: null, runtimeStopped: false };
    },
  };
</script>
<main class="fixture">
  <p>测试专用 mock gateway；不执行真实 VPN、配置删除或平台停止。</p>
  <Button onclick={() => { open = true; }}>重新打开测试删除对话框</Button>
  <output id="profile-removal-calls" aria-label="测试调用">{JSON.stringify(calls)}</output>
  <output id="profile-removal-outcomes" aria-label="已确认删除结果">{JSON.stringify(outcomes)}</output>
</main>
<ProfileRemovalDialog {targetId} bind:open {gateway} onremoved={(result) => { outcomes = [...outcomes, result]; open = false; }} />
<style>
  .fixture { max-width: 404px; min-width: 0; margin: auto; padding: 12px; display: grid; gap: 12px; }
  output { min-width: 0; overflow-wrap: anywhere; word-break: break-word; font-size: 12px; }
</style>
