<script lang="ts">
  import { onMount } from 'svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
  import { listProxyConfigs, setActiveProxyConfig, type ProxyConfigProfile } from '$lib/services/config';
  import { getAppErrorMessage, getGuiStackStatus } from '$lib/services/core';
  import { requireCommandSuccess } from '$lib/services/command-result';
  import type { GuiFeatureStatus } from '$lib/types/gui-api';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import ProfessionalOverview from './ProfessionalOverview.svelte';
  import TunControl from './TunControl.svelte';
  import type { OverviewModel, Destination } from './model';
  import { OverviewOperations } from './operations.svelte';
  import type { OverviewActions } from './types';

  const operations = new OverviewOperations();
  let { model, trafficUnavailable }: { model: OverviewModel; trafficUnavailable: string | null } = $props();
  let profiles = $state<ProxyConfigProfile[]>([]);
  let profilesLoading = $state(true);
  let profilesError = $state<string | null>(null);
  let stack = $state<GuiFeatureStatus | null>(null);
  let view = $state<{ showTunDetails: () => void }>();
  let profileRequest = 0;
  let stackRequest = 0;
  let disposed = false;
  const ready = $derived(model.ready);
  const externalBusy = $derived(guiState.isCoreBusy || guiState.isSwitchingTun || guiState.isSwitchingSystemProxy || guiState.isSwitchingMode || guiState.isSelectingPolicy || guiState.isConnecting || guiState.isDisconnecting || guiState.isInitializing);
  const busy = $derived(!!operations.feedback.pending || externalBusy);
  const selectedProfile = $derived(profiles.find((profile) => profile.active)?.id ?? '');
  const profileView = $derived({ selected: selectedProfile, loading: profilesLoading, error: profilesError, options: profiles.map((profile) => ({ value: profile.id, label: profile.name })) });
  const network = $derived({ ip: guiState.networkProbe?.ip ?? '', description: [guiState.networkProbe?.country, guiState.networkProbe?.region, guiState.networkProbe?.city, guiState.networkProbe?.isp ?? guiState.networkProbe?.org].filter(Boolean).join(' · '), loading: guiState.networkProbeLoading, error: guiState.networkProbeError });
  const canDisableTun = $derived(guiState.isTunSwitchOn && guiState.canDisableTun);
  const stackReady = $derived(model.ready && stack?.enabled === true);
  const stackLabel = $derived(!model.ready || !stack ? '待确认' : !stack.supported ? '不支持' : stack.enabled ? coreEvents.stackMode ?? '已启动' : stack.reason || '未启动');

  async function loadProfiles() {
    const request = ++profileRequest;
    profilesLoading = true;
    try {
      const result = await listProxyConfigs();
      if (request === profileRequest && !disposed) { profiles = result; profilesError = null; }
      return true;
    } catch (error) {
      if (request === profileRequest && !disposed) profilesError = getAppErrorMessage(error, '读取配置列表失败');
      return false;
    } finally { if (request === profileRequest && !disposed) profilesLoading = false; }
  }
  async function loadStack() {
    const request = ++stackRequest;
    try { const result = await getGuiStackStatus(); if (request === stackRequest && !disposed) stack = result; }
    catch { if (request === stackRequest && !disposed) stack = null; }
  }
  onMount(() => {
    void loadProfiles();
    void guiState.refreshSelfTest();
    const unsubscribe = proxyConfigSignal.onActiveChanged(() => { void loadProfiles(); });
    const checks = window.setInterval(() => { if (document.visibilityState !== 'hidden') void guiState.refreshSelfTest(); }, 30_000);
    return () => { disposed = true; profileRequest++; stackRequest++; operations.destroy(); unsubscribe(); window.clearInterval(checks); };
  });
  $effect(() => { void coreEvents.statusTick; if (ready) void loadStack(); else stack = null; });

  function navigate(target: Destination) {
    if (target === 'nodes' || target === 'profiles' || target === 'connections') store.activeTab = target;
    else store.openSettings(target);
  }
  function run(target: string, action: () => Promise<string>) { if (!busy) void operations.run(target, action); }
  const actions: OverviewActions = {
    navigate,
    refresh: () => run('checks', async () => {
      await Promise.all([guiState.refreshAll(), guiState.probeNetwork(), loadProfiles(), loadStack()]);
      const errors = [guiState.connectionError, guiState.tunStatusError, guiState.policyGroupsError, guiState.networkProbeError, profilesError, !guiState.selfTest ? '未取得就绪检查结果' : null].filter(Boolean);
      if (errors.length) throw new Error(errors.join('；'));
      return '检查已完成，请以最新状态与异常提示为准';
    }),
    chooseProfile: (id) => {
      if (!id || id === selectedProfile || !profiles.some((profile) => profile.id === id)) return;
      run('profile', async () => {
        let committed = false;
        try {
          const profile = await setActiveProxyConfig(id);
          committed = true;
          // The command response is authoritative even if a later list read fails.
          if (!disposed) profiles = profiles.map((item) => item.id === id ? profile : { ...item, active: false });
          const [listed] = await Promise.all([loadProfiles(), guiState.refreshNodeStateAfterConfigChange(), guiState.refreshAll()]);
          if (!listed) throw new Error('配置已切换，但列表刷新失败，请重新检查');
          return `已切换到 ${profile.name}`;
        } catch (error) {
          if (!committed) await Promise.all([loadProfiles(), guiState.refreshAll()]);
          throw new Error(getAppErrorMessage(error, '切换配置失败'));
        }
      });
    },
    choosePolicy: (group, target) => {
      if (!model.groupsReady || guiState.policyGroups.find((item) => item.name === group)?.selected === target) return;
      run(`policy:${group}`, async () => { requireCommandSuccess(await guiState.selectPolicy(group, target)); return `已切换到 ${target}`; });
    },
    setMode: (mode) => {
      if (!model.ready || model.mode === mode) return;
      run('mode', async () => { requireCommandSuccess(await guiState.setProxyMode(mode)); return '代理模式已生效'; });
    },
    toggleTun: () => run('tun', async () => { requireCommandSuccess(await guiState.toggleTun()); return guiState.isTunSwitchOn ? 'TUN 已开启，请查看接管健康状态' : 'TUN 与自动恢复已关闭'; }),
  };
  function toggleSystemProxy() { run('system-proxy', async () => { requireCommandSuccess(await guiState.toggleSystemProxy()); return guiState.isSystemProxyEnabled ? '系统代理已开启' : '系统代理已关闭'; }); }
</script>

<ProfessionalOverview bind:this={view} {model} profiles={profileView} {network} feedback={operations.feedback} {actions} {busy} refreshing={operations.feedback.pending === 'checks'} {canDisableTun}>
  {#snippet core()}<CoreStatusCard {busy} stateUnknown={model.stale} onToggleSystemProxy={toggleSystemProxy} />{/snippet}
  {#snippet tun()}<TunControl {model} onInspect={() => view?.showTunDetails()} onToggle={actions.toggleTun} switchOn={guiState.isTunSwitchOn} canToggle={!busy && (guiState.isTunSwitchOn ? guiState.canDisableTun : model.tunConfirmed && guiState.canEnableTun)} switching={guiState.isSwitchingTun} {stackLabel} {stackReady} feedback={operations.feedback} />{/snippet}
  {#snippet traffic()}<TrafficChart history={overviewData.speedHistory} unsupported={!guiState.supportsTrafficStats} unavailableReason={trafficUnavailable} />{/snippet}
</ProfessionalOverview>
