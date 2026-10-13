<script lang="ts">
  import MobileOverview from '$lib/components/mobile/MobileOverview.svelte';
  import MobileShell from '$lib/components/mobile/MobileShell.svelte';
  import MobileShellPageFixture from './MobileShellPageFixture.svelte';
  import { provideTrafficWorkspace, useTrafficWorkspace } from '$lib/features/traffic/context';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  provideTrafficWorkspace();
  const workspace = useTrafficWorkspace();
  workspace.statistics = { kind: 'endpoint', plane: 'outer', query: 'previous search', offset: 24 };
  store.activeTab = 'overview';
  store.mobilePresentation = true;
  mobilePlatform.isMobileLayout = true;
  mobilePlatform.responsivePreview = true;
  Object.assign(guiState, {
    connection: { coreAvailable: true }, connectionUpdatedAt: Date.now(), connectionError: null,
    proxyMode: { currentMode: 'rule', availableModes: ['rule', 'direct', 'global'] },
    configNodes: [{ tag: 'node-a' }], configPolicyGroups: [], policyGroups: [],
  });
</script>
<MobileShell>
  {#snippet overview()}<MobileOverview />{/snippet}
  {#snippet nodes()}<MobileShellPageFixture name="节点" />{/snippet}
  {#snippet configuration()}<MobileShellPageFixture name="配置" />{/snippet}
</MobileShell>
