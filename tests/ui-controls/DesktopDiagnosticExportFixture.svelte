<script lang="ts">
  import { onDestroy } from 'svelte';
  import ErrorRecoveryActions from '../../src/lib/components/core/ErrorRecoveryActions.svelte';
  import { store } from '$lib/services/store.svelte';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import DesktopDiagnosticExport from '../../src/lib/components/settings/DesktopDiagnosticExport.svelte';
  import type { DiagnosticPreview } from '../../src/lib/services/native-diagnostics';
  const mode = new URLSearchParams(location.search).get('mode');
  let navigation = $state<string[]>([]);
  mobilePlatform.runtimePlatform = 'desktop';
  const originalOpenSettings = store.openSettings;
  store.openSettings = (section) => { navigation = [...navigation, section ?? 'core']; };
  onDestroy(() => { store.openSettings = originalOpenSettings; });
  let previews = $state(0);
  let deliveries = $state<string[]>([]);
  let saved = $state(0);
  let discarded = $state<string[]>([]);
  let release: (() => void) | null = null;
  Object.assign(window, { releaseDesktopDiagnosticExport: () => release?.() });
  const gateway = {
    async preview(): Promise<DiagnosticPreview> {
      previews++;
      if (mode === 'preview-pending') await new Promise<void>((resolve) => release = resolve);
      if (mode === 'preview-fail') throw new Error('untrusted-error-canary');
      return {
        planId: `fixed-preview-${previews}`, createdAtUnixMs: '1000', expiresAtUnixMs: '301000', bytes: 235,
        scope: { profileId: 'profile-original', configRevision: '7', clientInstanceId: '9' },
        logCount: 2, frameCount: null, taskCount: 1,
        omissions: [{ category: 'rawLogFields', count: 2, reason: '自由字段没有可信分类，已省略' }],
      };
    },
    async discard(planId: string) { discarded = [...discarded, planId]; },
    async deliver(planId: string) {
      deliveries = [...deliveries, planId];
      if (mode === 'pending') await new Promise<void>((resolve) => release = resolve);
      if (mode === 'rejected') throw { code: 'conflict', message: 'untrusted-error-canary' };
      if (mode === 'failure') throw new Error('untrusted-error-canary');
      return { directory: '/fixture/diagnostics', files: ['diagnostics.json'], createdAtUnixMs: 1000 };
    },
  };
</script>
<section aria-label="桌面诊断出口实际组件测试">
  <ErrorRecoveryActions />
  <DesktopDiagnosticExport {gateway} onexported={() => { saved++; }} />
  <output aria-label="恢复入口导航">{JSON.stringify(navigation)}</output>
  <output aria-label="诊断预览次数">{previews}</output>
  <output aria-label="诊断交付计划">{JSON.stringify(deliveries)}</output>
  <output aria-label="已放弃计划">{JSON.stringify(discarded)}</output>
  <output aria-label="已确认保存次数">{saved}</output>
</section>
