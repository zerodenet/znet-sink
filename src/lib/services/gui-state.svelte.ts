import {
  getGuiSelfTestSnapshot,
  getGuiConnectionStatus,
  guiConnect,
  guiDisconnect,
  startCoreProcess,
  restartCoreProcess,
  enableSystemProxy as enableSystemProxyCommand,
  disableSystemProxy as disableSystemProxyCommand,
  getGuiProxyModeStatus,
  guiSetProxyMode,
  getGuiCoreOverview,
  getGuiPolicyGroups,
  getConfigProxyNodes,
  getConfigPolicyGroups,
  getGuiZeroCapabilities,
  guiNetworkProbe,
  type NetworkProbeResult,
  trayUpdateStatus,
} from './core';
import { getAppConfig } from './core';
import { getGuiTunStatus, enableGuiTun, disableGuiTun } from './tun';
import { error as toastError, success as toastSuccess, warning as toastWarning } from './toast.svelte';
import { tracedOperation } from './telemetry';
import { createLatestRequestGate } from './latest-request-gate.js';
import {
  retainConfiguredPolicyGroups,
  shouldApplyPolicyProbeEvent,
} from './node-state-reconcile';
import type {
  ConfigProxyNode,
  SelfTestSnapshot,
  ConnectionStatus,
  ProxyModeStatus,
  CoreOverview,
  PolicyGroup,
  ProxyMode,
  GuiTunStatus,
} from '$lib/types/gui-api';

const NETWORK_PROBE_INTERVAL_MS = 5 * 60_000;

class GuiStateStore {
  selfTest = $state<SelfTestSnapshot | null>(null);
  connection = $state<ConnectionStatus | null>(null);
  proxyMode = $state<ProxyModeStatus | null>(null);
  coreOverview = $state<CoreOverview | null>(null);
  policyGroups = $state<PolicyGroup[]>([]);
  tunStatus = $state<GuiTunStatus | null>(null);
  configNodes = $state<ConfigProxyNode[]>([]);
  configPolicyGroups = $state<PolicyGroup[]>([]);
  networkProbe = $state<NetworkProbeResult | null>(null);
  networkProbeLoading = $state(false);
  networkProbeError = $state<string | null>(null);

  supportsTrafficStats = $state(true);

  isInitializing = $state(true);
  isLoading = $state(false);
  isConnecting = $state(false);
  isDisconnecting = $state(false);
  isStartingCore = $state(false);
  isStoppingCore = $state(false);
  isSwitchingSystemProxy = $state(false);
  isSwitchingTun = $state(false);
  isSwitchingMode = $state(false);

  private isInitialized = false;
  private lastStatusTick = -1;
  private networkProbeTimer: ReturnType<typeof setInterval> | null = null;
  private networkProbePending = false;
  private internetSharingWarningShown = false;
  private configNodesRefreshGate = createLatestRequestGate();
  private configPolicyGroupsRefreshGate = createLatestRequestGate();
  private policyGroupsRefreshGate = createLatestRequestGate();

  async initialize() {
    if (this.isInitialized) return;
    this.isInitialized = true;
    this.isInitializing = true;

    this.startPeriodicNetworkProbe();
    void this.probeNetwork();
    await this.refreshAll();

    // The first authoritative snapshot is complete. UI action guards may now
    // be evaluated normally, including the mode-specific auto-connect below.
    this.isInitializing = false;

    try {
      const appConfig = await getAppConfig();
      if (appConfig.core.autoConnect) {
        await this.autoConnectForMode(appConfig.ui.uiMode === 'lite' ? 'lite' : 'pro');
      }
    } catch {
      // Configuration errors must not prevent the rest of the UI from loading.
    }
  }

  private async autoConnectForMode(mode: 'lite' | 'pro') {
    if (!this.connection?.coreAvailable) {
      // Kernel startup is asynchronous in Tauri. Give it one short retry, but
      // never make autoConnect itself responsible for starting/stopping Zero.
      await new Promise((resolve) => setTimeout(resolve, 1200));
      await Promise.allSettled([this.refreshConnectionStatus(), this.refreshTunStatus()]);
    }
    if (!this.connection?.coreAvailable) return;

    if (mode === 'lite') {
      if (!this.isTunEnabled) await this.connect();
      return;
    }

    if (this.connection.systemProxyEnabled === true) return;
    this.isConnecting = true;
    try {
      this.connection = await tracedOperation('proxy', 'connection.auto_connect', () => guiConnect());
      this.syncTrayStatus();
      await this.refreshPolicyPanels();
      await this.refreshSelfTest();
    } catch {
      await this.refreshConnectionStatus();
    } finally {
      this.isConnecting = false;
    }
  }

  async refreshAll() {
    await Promise.allSettled([
      this.refreshSelfTest(),
      this.refreshConnectionStatus(),
      this.refreshProxyMode(),
      this.refreshCoreOverview(),
      this.refreshConfigNodes(),
      this.refreshConfigPolicyGroups(),
      this.refreshPolicyGroups(),
      this.refreshTunStatus(),
      this.refreshCapabilities(),
    ]);
  }

  refreshOnTick(tick: number) {
    if (tick > 0 && tick !== this.lastStatusTick) {
      this.lastStatusTick = tick;
      void this.refreshRuntimeState();
    }
  }

  async refreshSelfTest() {
    try {
      const snapshot = await getGuiSelfTestSnapshot();
      this.selfTest = snapshot;
      const internetSharingWarning = snapshot.checks.some(
        (check) => check.key === 'internetSharing' && check.status === 'warn',
      );
      if (internetSharingWarning && !this.internetSharingWarningShown) {
        toastWarning('检测到 Windows 热点或网络共享；其他设备不会自动使用本机系统代理。');
      }
      this.internetSharingWarningShown = internetSharingWarning;
    } catch {
      this.selfTest = null;
    }
  }

  async refreshConnectionStatus() {
    try {
      this.connection = await getGuiConnectionStatus();
      this.syncTrayStatus();
    } catch {
      // Preserve the last trusted ownership snapshot through a transient IPC
      // failure instead of making PID/proxy state flicker.
    }
  }

  async refreshProxyMode() {
    try {
      this.proxyMode = await getGuiProxyModeStatus();
    } catch {
      this.proxyMode = null;
    }
  }

  async refreshCoreOverview() {
    try {
      this.coreOverview = await getGuiCoreOverview();
    } catch {
      this.coreOverview = null;
    }
  }

  async refreshConfigNodes() {
    const request = this.configNodesRefreshGate.begin();
    try {
      const nodes = await getConfigProxyNodes();
      if (this.configNodesRefreshGate.canApply(request)) {
        this.configNodes = nodes;
      }
    } catch {
      // Keep the last known-good config snapshot during a config reload.
    }
  }

  async refreshConfigPolicyGroups() {
    const request = this.configPolicyGroupsRefreshGate.begin();
    try {
      const groups = await getConfigPolicyGroups();
      if (this.configPolicyGroupsRefreshGate.canApply(request)) {
        this.configPolicyGroups = groups;
      }
    } catch {
      // Preserve the previous snapshot until a newer request succeeds.
    }
  }

  async refreshPolicyGroups() {
    const request = this.policyGroupsRefreshGate.begin();
    try {
      const groups = await getGuiPolicyGroups();
      if (this.policyGroupsRefreshGate.canApply(request)) {
        this.policyGroups = groups;
      }
    } catch (e: any) {
      console.warn('[gui-state] policy groups failed:', this.errorMessage(e));
    }
  }

  async refreshNodeStateAfterConfigChange() {
    this.configNodesRefreshGate.reset();
    this.configPolicyGroupsRefreshGate.reset();
    this.policyGroupsRefreshGate.reset();

    await Promise.allSettled([
      this.refreshConfigNodes(),
      this.refreshConfigPolicyGroups(),
    ]);

    this.policyGroups = retainConfiguredPolicyGroups(this.policyGroups, this.configPolicyGroups);
    await this.refreshPolicyGroups();
  }

  applyPolicyProbeCompleted(event: import('$lib/types/gui-api').PolicyProbeCompletedEvent) {
    const existing = this.policyGroups.find((group) => group.name === event.policyTag);
    if (!shouldApplyPolicyProbeEvent(this.configPolicyGroups, this.policyGroups, event.policyTag)) return;
    this.policyGroupsRefreshGate.reset();
    const previousMembers = new Map(existing?.outbounds.map((member) => [member.tag, member]) ?? []);
    const outbounds = event.members.map((member) => ({
      ...previousMembers.get(member.tag),
      ...member,
      lastCheckedUnixMs: member.lastCheckedUnixMs ?? event.completedAtUnixMs,
    }));
    const updated = {
      ...existing,
      name: event.policyTag,
      kind: existing?.kind ?? 'url_test',
      selected: event.selected ?? existing?.selected,
      outbounds,
    };
    this.policyGroups = existing
      ? this.policyGroups.map((group) => group.name === event.policyTag ? updated : group)
      : [...this.policyGroups, updated];
  }

  async refreshTunStatus() {
    try {
      this.tunStatus = await getGuiTunStatus();
      this.syncTrayStatus();
    } catch {
      // Keep the last trusted TUN state through a short kernel transition.
    }
  }

  async refreshCapabilities() {
    try {
      const caps = await getGuiZeroCapabilities();
      const features = caps?.features ?? [];
      this.supportsTrafficStats =
        caps.available && (features.includes('query') || features.includes('runtime_snapshot'));
    } catch {
      // Kernel not connected yet; keep the optimistic default.
    }
  }

  private async refreshRuntimeState() {
    await Promise.allSettled([
      this.refreshConnectionStatus(),
      this.refreshCoreOverview(),
      this.refreshConfigNodes(),
      this.refreshPolicyGroups(),
      this.refreshTunStatus(),
      this.refreshCapabilities(),
    ]);
  }

  private async refreshPolicyPanels() {
    await Promise.allSettled([
      this.refreshProxyMode(),
      this.refreshCoreOverview(),
      this.refreshPolicyGroups(),
    ]);
  }

  private async refreshModeState() {
    await Promise.allSettled([
      this.refreshConnectionStatus(),
      this.refreshProxyMode(),
      this.refreshCoreOverview(),
      this.refreshPolicyGroups(),
      this.refreshTunStatus(),
      this.refreshCapabilities(),
    ]);
  }

  private errorMessage(e: any): string {
    return e?.message ?? e ?? '未知错误';
  }

  private syncTrayStatus() {
    void trayUpdateStatus(this.isProcessRunning, this.isCaptureEnabled).catch(() => {});
  }

  async probeNetwork() {
    if (this.networkProbeLoading) {
      this.networkProbePending = true;
      return;
    }
    this.networkProbeLoading = true;
    this.networkProbePending = false;
    this.networkProbeError = null;
    try {
      this.networkProbe = await guiNetworkProbe();
    } catch (error) {
      this.networkProbe = null;
      this.networkProbeError = this.errorMessage(error);
    } finally {
      this.networkProbeLoading = false;
      void this.refreshSelfTest();
      if (this.networkProbePending && this.isInitialized) {
        void this.probeNetwork();
      }
    }
  }

  /** Compact-mode power lifecycle: Zero TUN on/off, Zero process stays alive. */
  async connect() {
    if (!this.canConnect) return;
    this.isConnecting = true;
    this.isSwitchingTun = true;
    try {
      this.tunStatus = await tracedOperation('proxy', 'tun.enable', () => enableGuiTun());
      this.syncTrayStatus();
      toastSuccess('代理已开启');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`连接失败: ${this.errorMessage(e)}`);
      await Promise.allSettled([this.refreshTunStatus(), this.refreshConnectionStatus()]);
    } finally {
      this.isSwitchingTun = false;
      this.isConnecting = false;
    }
  }

  async disconnect() {
    if (!this.canDisconnect) return;
    this.isDisconnecting = true;
    this.isSwitchingTun = true;
    try {
      this.tunStatus = await tracedOperation('proxy', 'tun.disable', () => disableGuiTun());
      this.syncTrayStatus();
      toastSuccess('代理已关闭，内核保持运行');
      await this.refreshPolicyPanels();
    } catch (e: any) {
      toastError(`断开失败: ${this.errorMessage(e)}`);
      await this.refreshTunStatus();
    } finally {
      this.isSwitchingTun = false;
      this.isDisconnecting = false;
    }
  }

  async prepareLiteCapture() {
    await Promise.allSettled([this.refreshConnectionStatus(), this.refreshTunStatus()]);
    const systemProxyOwned = this.connection?.systemProxyEnabled === true;

    if (this.isTunEnabled) {
      if (systemProxyOwned) {
        this.connection = await tracedOperation('proxy', 'lite.system_proxy.release', () => guiDisconnect());
      }
      this.syncTrayStatus();
      return;
    }

    if (!systemProxyOwned) return;

    this.isSwitchingTun = true;
    this.isDisconnecting = true;
    let tunStarted = false;
    try {
      this.tunStatus = await tracedOperation('proxy', 'lite.tun.handoff', () => enableGuiTun());
      tunStarted = this.tunStatus.enabled;
      if (!tunStarted) throw new Error('Zero 未确认 TUN 已启动');

      // Only release the GUI-owned system proxy after TUN is confirmed. This
      // keeps a working capture path until the replacement is actually ready.
      this.connection = await tracedOperation('proxy', 'lite.system_proxy.release', () => guiDisconnect());
      await this.refreshModeState();
      this.syncTrayStatus();
    } catch (error) {
      if (tunStarted) {
        try {
          this.tunStatus = await disableGuiTun();
        } catch {
          // Preserve the handoff error; refresh below exposes rollback state.
        }
      }
      await Promise.allSettled([this.refreshTunStatus(), this.refreshConnectionStatus()]);
      throw error;
    } finally {
      this.isDisconnecting = false;
      this.isSwitchingTun = false;
    }
  }

  async startCore() {
    if (!this.canStartCore) return;
    this.isStartingCore = true;
    try {
      await tracedOperation('kernel', 'kernel.start', () => startCoreProcess());
      toastSuccess('内核监听已启动');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`启动内核失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStartingCore = false;
    }
  }

  async restartCore() {
    if (!this.canRestartCore) return;
    this.isStoppingCore = true;
    try {
      await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess());
      toastSuccess('内核已重启');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`重启内核失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStoppingCore = false;
    }
  }

  async enableSystemProxy() {
    if (!this.canEnableSystemProxy) return;
    this.isSwitchingSystemProxy = true;
    try {
      await tracedOperation('proxy', 'system_proxy.enable', () => enableSystemProxyCommand());
      toastSuccess('系统代理已开启');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`开启系统代理失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async disableSystemProxy() {
    if (!this.canDisableSystemProxy) return;
    this.isSwitchingSystemProxy = true;
    try {
      await tracedOperation('proxy', 'system_proxy.disable', () => disableSystemProxyCommand());
      toastSuccess('系统代理已关闭');
      await this.refreshConnectionStatus();
    } catch (e: any) {
      toastError(`关闭系统代理失败: ${this.errorMessage(e)}`);
      await this.refreshConnectionStatus();
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async toggleSystemProxy() {
    if (this.connection?.systemProxyEnabled === true) {
      await this.disableSystemProxy();
    } else {
      await this.enableSystemProxy();
    }
  }

  async enableTun() {
    if (!this.canEnableTun) return;
    this.isSwitchingTun = true;
    try {
      this.tunStatus = await enableGuiTun();
      toastSuccess('TUN 已开启');
      await this.refreshRuntimeState();
    } catch (e: any) {
      toastError(`开启 TUN 失败: ${this.errorMessage(e)}`);
      await this.refreshTunStatus();
      await this.refreshConnectionStatus();
    } finally {
      this.isSwitchingTun = false;
    }
  }

  async disableTun() {
    if (!this.canDisableTun) return;
    this.isSwitchingTun = true;
    try {
      this.tunStatus = await disableGuiTun();
      toastSuccess('TUN 已关闭');
      await this.refreshTunStatus();
    } catch (e: any) {
      toastError(`关闭 TUN 失败: ${this.errorMessage(e)}`);
      await this.refreshTunStatus();
    } finally {
      this.isSwitchingTun = false;
    }
  }

  async toggleTun() {
    if (this.isTunEnabled) await this.disableTun();
    else await this.enableTun();
  }

  async setProxyMode(mode: ProxyMode) {
    this.isSwitchingMode = true;
    try {
      this.proxyMode = await guiSetProxyMode(mode);
      await this.refreshModeState();
    } catch (e: any) {
      toastError(`切换代理模式失败: ${this.errorMessage(e)}`);
      await this.refreshModeState();
    } finally {
      this.isSwitchingMode = false;
    }
  }

  destroy() {
    this.isInitialized = false;
    this.networkProbePending = false;
    this.stopPeriodicNetworkProbe();
  }

  private startPeriodicNetworkProbe() {
    if (this.networkProbeTimer) return;
    this.networkProbeTimer = setInterval(() => {
      if (this.isInitialized) void this.probeNetwork();
    }, NETWORK_PROBE_INTERVAL_MS);
  }

  private stopPeriodicNetworkProbe() {
    if (!this.networkProbeTimer) return;
    clearInterval(this.networkProbeTimer);
    this.networkProbeTimer = null;
  }

  get isCaptureEnabled(): boolean {
    return this.isTunEnabled || this.connection?.systemProxyEnabled === true;
  }

  /** Existing Lite views use this as their power state. */
  get isConnected(): boolean {
    return this.isTunEnabled;
  }

  /** Compatibility alias for the merged Lite overview/session boundary. */
  get isSystemProxyEnabled(): boolean {
    return this.isTunEnabled;
  }

  get isTunEnabled(): boolean {
    return this.tunStatus?.enabled === true;
  }

  get isProcessRunning(): boolean {
    return this.connection?.coreAvailable === true || this.connection?.processState === 'running';
  }

  get isManagedProcessRunning(): boolean {
    return this.connection?.processState === 'running';
  }

  get isCoreBusy(): boolean {
    return this.isStartingCore
      || this.isStoppingCore
      || this.connection?.processState === 'starting'
      || this.connection?.processState === 'stopping';
  }

  get canConnect(): boolean {
    if (this.isInitializing) return false;
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    const missingProxyConfig = this.selfTest !== null && !this.selfTest.activeProxyConfigId;
    return (!selfTestBlocking || this.isProcessRunning)
      && !missingProxyConfig
      && !this.isConnecting
      && !this.isDisconnecting
      && !this.isSwitchingTun
      && !this.isTunEnabled;
  }

  get canDisconnect(): boolean {
    if (this.isInitializing) return false;
    return !this.isConnecting && !this.isDisconnecting && !this.isSwitchingTun && this.isTunEnabled;
  }

  get canStartCore(): boolean {
    if (this.isInitializing) return false;
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return !selfTestBlocking
      && !this.isCoreBusy
      && !this.isConnecting
      && !this.isDisconnecting
      && !this.isProcessRunning;
  }

  get canRestartCore(): boolean {
    return !this.isCoreBusy && !this.isConnecting && !this.isDisconnecting && this.isManagedProcessRunning;
  }

  get canEnableSystemProxy(): boolean {
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return (!selfTestBlocking || this.isProcessRunning)
      && !this.isSwitchingSystemProxy
      && !this.isConnecting
      && !this.isDisconnecting
      && this.connection?.systemProxyEnabled !== true;
  }

  get canDisableSystemProxy(): boolean {
    return !this.isSwitchingSystemProxy
      && !this.isConnecting
      && !this.isDisconnecting
      && this.connection?.systemProxyEnabled === true;
  }

  get canEnableTun(): boolean {
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return (!selfTestBlocking || this.isProcessRunning)
      && !this.isSwitchingTun
      && !this.isConnecting
      && !this.isDisconnecting
      && !this.isTunEnabled;
  }

  get canDisableTun(): boolean {
    return !this.isSwitchingTun && !this.isConnecting && !this.isDisconnecting && this.isTunEnabled;
  }

  get blockingIssues(): string[] {
    return this.selfTest?.blockingIssues ?? [];
  }
}

export const guiState = new GuiStateStore();
