import {
  getGuiSelfTestSnapshot,
  getGuiConnectionStatus,
  guiConnect,
  guiDisconnect,
  startCoreProcess,
  restartCoreProcess,
  enableSystemProxy as enableSystemProxyCommand,
  disableSystemProxy as disableSystemProxyCommand,
  getGuiTunStatus,
  enableGuiTun,
  disableGuiTun,
  getGuiProxyModeStatus,
  guiSetProxyMode,
  getGuiCoreOverview,
  getGuiPolicyGroups,
  getConfigProxyNodes,
  getConfigPolicyGroups,
  getGuiZeroCapabilities,
  trayUpdateStatus,
} from './core';
import { error as toastError, success as toastSuccess } from './toast.svelte';
import { coreEvents } from './core-events.svelte';
import type {
  ConfigProxyNode,
  SelfTestSnapshot,
  ConnectionStatus,
  ProxyModeStatus,
  CoreOverview,
  PolicyGroup,
  ProxyMode,
  GuiFeatureStatus,
} from '$lib/types/gui-api';

class GuiStateStore {
  selfTest = $state<SelfTestSnapshot | null>(null);
  connection = $state<ConnectionStatus | null>(null);
  proxyMode = $state<ProxyModeStatus | null>(null);
  coreOverview = $state<CoreOverview | null>(null);
  policyGroups = $state<PolicyGroup[]>([]);
  tunStatus = $state<GuiFeatureStatus | null>(null);
  configNodes = $state<ConfigProxyNode[]>([]);
  configPolicyGroups = $state<PolicyGroup[]>([]);

  // Whether the kernel supports live traffic stats (needs "query" or
  // "runtime-snapshot" capability). When false, the traffic chart shows
  // a downgrade hint instead of silently reading 0.
  supportsTrafficStats = $state(true);

  isInitializing = $state(true); // true until first refreshAll completes
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

  async initialize() {
    if (this.isInitialized) return;
    this.isInitialized = true;
    this.isInitializing = true;

    await this.refreshAll();

    // Unlock kernel action buttons after the first full state snapshot.
    // Until this point the UI may show stale pre-load state where buttons
    // look clickable but the kernel is already running or starting.
    this.isInitializing = false;
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

  /** Refresh connection-related runtime state when core status ticks change. */
  refreshOnTick(tick: number) {
    if (tick > 0 && tick !== this.lastStatusTick) {
      this.lastStatusTick = tick;
      void this.refreshRuntimeState();
    }
  }

  async refreshSelfTest() {
    try {
      this.selfTest = await getGuiSelfTestSnapshot();
    } catch {
      this.selfTest = null;
    }
  }

  async refreshConnectionStatus() {
    try {
      this.connection = await getGuiConnectionStatus();
      this.syncTrayStatus();
    } catch {
      this.connection = null;
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
    try {
      this.configNodes = await getConfigProxyNodes();
    } catch {
      this.configNodes = [];
    }
  }

  async refreshConfigPolicyGroups() {
    try {
      this.configPolicyGroups = await getConfigPolicyGroups();
    } catch {
      this.configPolicyGroups = [];
    }
  }

  async refreshPolicyGroups() {
    try {
      const groups = await getGuiPolicyGroups();
      console.warn('[gui-state] policy groups loaded:', groups.length, 'groups');
      this.policyGroups = groups;
    } catch (e: any) {
      console.warn('[gui-state] policy groups failed:', this.errorMessage(e));
      this.policyGroups = [];
    }
  }

  async refreshTunStatus() {
    try {
      this.tunStatus = await getGuiTunStatus();
    } catch {
      this.tunStatus = null;
    }
  }

  /** Probe kernel capabilities to determine feature support such as traffic stats. */
  async refreshCapabilities() {
    try {
      const caps = await getGuiZeroCapabilities();
      const features = caps?.features ?? [];
      this.supportsTrafficStats =
        caps.available && (features.includes('query') || features.includes('runtime-snapshot'));
    } catch {
      // Kernel not connected yet; keep the optimistic default.
    }
  }

  private async refreshRuntimeState() {
    await Promise.allSettled([
      this.refreshConnectionStatus(),
      this.refreshCoreOverview(),
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
    return e?.message ?? e ?? '\u672a\u77e5\u9519\u8bef';
  }

  /**
   * Mirror the current connection/process state onto the system-tray icon
   * so the tray stays in sync even when the window is hidden.
   */
  private syncTrayStatus() {
    void trayUpdateStatus(this.isProcessRunning, this.isConnected).catch(() => {});
  }

  async connect() {
    this.isConnecting = true;
    try {
      this.connection = await guiConnect();
      this.syncTrayStatus();
      toastSuccess('\u7cfb\u7edf\u4ee3\u7406\u5df2\u5f00\u542f\uff0c\u670d\u52a1\u5df2\u751f\u6548');
      coreEvents.start();
      await this.refreshPolicyPanels();
    } catch (e: any) {
      toastError(`\u8fde\u63a5\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshConnectionStatus();
    } finally {
      this.isConnecting = false;
    }
  }

  async disconnect() {
    this.isDisconnecting = true;
    try {
      this.connection = await guiDisconnect();
      this.syncTrayStatus();
      toastSuccess('\u7cfb\u7edf\u4ee3\u7406\u5df2\u5173\u95ed\uff0c\u5185\u6838\u4fdd\u6301\u8fd0\u884c');
      await this.refreshPolicyPanels();
    } catch (e: any) {
      toastError(`\u65ad\u5f00\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshConnectionStatus();
    } finally {
      this.isDisconnecting = false;
    }
  }

  async startCore() {
    if (!this.canStartCore) return;
    this.isStartingCore = true;
    try {
      await startCoreProcess();
      toastSuccess('\u5185\u6838\u76d1\u542c\u5df2\u542f\u52a8');
      coreEvents.start();
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`\u542f\u52a8\u5185\u6838\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStartingCore = false;
    }
  }

  /** Restart the managed kernel by stopping it and starting it again immediately. */
  async restartCore() {
    if (!this.canRestartCore) return;
    this.isStoppingCore = true;
    try {
      await restartCoreProcess();
      toastSuccess('\u5185\u6838\u5df2\u91cd\u542f');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`\u91cd\u542f\u5185\u6838\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStoppingCore = false;
    }
  }

  async enableSystemProxy() {
    if (!this.canEnableSystemProxy) return;
    this.isSwitchingSystemProxy = true;
    try {
      await enableSystemProxyCommand();
      toastSuccess('\u7cfb\u7edf\u4ee3\u7406\u5df2\u5f00\u542f');
      await this.refreshRuntimeState();
    } catch (e: any) {
      toastError(`\u5f00\u542f\u7cfb\u7edf\u4ee3\u7406\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async disableSystemProxy() {
    if (!this.canDisableSystemProxy) return;
    this.isSwitchingSystemProxy = true;
    try {
      await disableSystemProxyCommand();
      toastSuccess('\u7cfb\u7edf\u4ee3\u7406\u5df2\u5173\u95ed');
      await this.refreshConnectionStatus();
    } catch (e: any) {
      toastError(`\u5173\u95ed\u7cfb\u7edf\u4ee3\u7406\u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshConnectionStatus();
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async toggleSystemProxy() {
    if (this.isSystemProxyEnabled) {
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
      toastSuccess('TUN \u5df2\u5f00\u542f');
      await this.refreshRuntimeState();
    } catch (e: any) {
      toastError(`\u5f00\u542f TUN \u5931\u8d25: ${this.errorMessage(e)}`);
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
      toastSuccess('TUN \u5df2\u5173\u95ed');
      await this.refreshTunStatus();
    } catch (e: any) {
      toastError(`\u5173\u95ed TUN \u5931\u8d25: ${this.errorMessage(e)}`);
      await this.refreshTunStatus();
    } finally {
      this.isSwitchingTun = false;
    }
  }

  async toggleTun() {
    if (this.isTunEnabled) {
      await this.disableTun();
    } else {
      await this.enableTun();
    }
  }

  async setProxyMode(mode: ProxyMode) {
    this.isSwitchingMode = true;
    try {
      this.proxyMode = await guiSetProxyMode(mode, true);
      // Mode switches restart the core, so refresh runtime state immediately
      // instead of waiting for the next status tick.
      await this.refreshModeState();
    } finally {
      this.isSwitchingMode = false;
    }
  }

  destroy() {
    this.isInitialized = false;
  }

  // Derived state

  get isConnected(): boolean {
    return this.connection?.state === 'connected';
  }

  get isSystemProxyEnabled(): boolean {
    return this.connection?.systemProxyEnabled === true;
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
      && !this.isConnected;
  }

  get canDisconnect(): boolean {
    if (this.isInitializing) return false;
    return !this.isConnecting && !this.isDisconnecting && this.isConnected;
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
      && !this.isSystemProxyEnabled;
  }

  get canDisableSystemProxy(): boolean {
    return !this.isSwitchingSystemProxy && !this.isConnecting && !this.isDisconnecting && this.isSystemProxyEnabled;
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

let lastGuiStateRefreshTick = -1;

$effect(() => {
  const tick = coreEvents.statusTick;
  if (tick > 0 && tick !== lastGuiStateRefreshTick) {
    lastGuiStateRefreshTick = tick;
    guiState.refreshOnTick(tick);
  }
});
