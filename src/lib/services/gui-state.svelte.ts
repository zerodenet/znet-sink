import {
  getGuiSelfTestSnapshot,
  getGuiConnectionStatus,
  guiConnect,
  guiDisconnect,
  startCoreProcess,
  stopCoreProcess,
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
  GuiFeatureStatus
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

  isInitializing = $state(true); // true until first refreshAll completes
  isLoading = $state(false);
  isConnecting = $state(false);
  isDisconnecting = $state(false);
  isStartingCore = $state(false);
  isStoppingCore = $state(false);
  isSwitchingSystemProxy = $state(false);
  isSwitchingTun = $state(false);
  isSwitchingMode = $state(false);

  /**
   * Bumped whenever the proxy mode (rule / global / direct) changes.
   * Node-page components watch this to re-sync their display after a
   * mode switch, since the kernel restart that follows invalidates the
   * cached runtime policy snapshot.
   */
  modeTick = $state(0);

  private isInitialized = false;
  private lastStatusTick = -1;

  async initialize() {
    if (this.isInitialized) return;
    this.isInitialized = true;
    this.isInitializing = true;

    await this.refreshAll();

    // Unlock kernel action buttons after the first full state snapshot.
    // Until this point the UI may show stale (pre-load) state where buttons
    // look clickable but the kernel is already running/starting.
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
    ]);
  }

  /** Refresh connection + overview on core process events. */
  refreshOnTick(tick: number) {
    if (tick > 0 && tick !== this.lastStatusTick) {
      this.lastStatusTick = tick;
      this.refreshConnectionStatus();
      this.refreshCoreOverview();
      this.refreshPolicyGroups();
      this.refreshTunStatus();
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
    } catch (e: any) {
      this.configNodes = [];
    }
  }

  async refreshConfigPolicyGroups() {
    try {
      this.configPolicyGroups = await getConfigPolicyGroups();
    } catch (e: any) {
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

  private async refreshRuntimeState() {
    await Promise.allSettled([
      this.refreshConnectionStatus(),
      this.refreshCoreOverview(),
      this.refreshPolicyGroups(),
      this.refreshTunStatus(),
    ]);
  }

  private errorMessage(e: any): string {
    return e?.message ?? e ?? '未知错误';
  }

  /**
   * Mirror the current connection/process state onto the system-tray icon
   * (tooltip + menu item enabled states). Called after every status
   * refresh so the tray stays in sync even when the window is hidden.
   * Best-effort: silently ignored outside Tauri.
   */
  private syncTrayStatus() {
    void trayUpdateStatus(this.isProcessRunning, this.isConnected).catch(() => {});
  }

  async connect() {
    this.isConnecting = true;
    try {
      this.connection = await guiConnect();
      this.syncTrayStatus();
      toastSuccess('系统代理已开启，服务已生效');
      coreEvents.start(); // re-subscribe after core is running
      await Promise.allSettled([
        this.refreshProxyMode(),
        this.refreshCoreOverview(),
        this.refreshPolicyGroups(),
      ]);
    } catch (e: any) {
      toastError(`连接失败: ${this.errorMessage(e)}`);
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
      toastSuccess('服务已关闭，内核已停止');
      await Promise.allSettled([
        this.refreshProxyMode(),
        this.refreshCoreOverview(),
        this.refreshPolicyGroups(),
      ]);
    } catch (e: any) {
      toastError(`断开失败: ${this.errorMessage(e)}`);
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
      toastSuccess('内核监听已启动');
      coreEvents.start(); // re-subscribe after core is running
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`启动内核失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStartingCore = false;
    }
  }

  async stopCore() {
    if (!this.canStopCore) return;
    this.isStoppingCore = true;
    try {
      await stopCoreProcess();
      toastSuccess('内核已停止');
      await this.refreshRuntimeState();
    } catch (e: any) {
      toastError(`停止内核失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
    } finally {
      this.isStoppingCore = false;
    }
  }

  /** Restart the managed kernel — stop then immediately start again. */
  async restartCore() {
    if (!this.canStopCore) return;
    this.isStoppingCore = true;
    try {
      await restartCoreProcess();
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
      await enableSystemProxyCommand();
      toastSuccess('系统代理已开启');
      await this.refreshRuntimeState();
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
      await disableSystemProxyCommand();
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
      // Mode switch restarts the core → refresh runtime state so the
      // node page, overview, and connection panels reflect the new mode
      // without waiting for the next status tick.
      await Promise.allSettled([
        this.refreshConnectionStatus(),
        this.refreshProxyMode(),
        this.refreshCoreOverview(),
        this.refreshPolicyGroups(),
        this.refreshTunStatus(),
      ]);
      this.modeTick++;
    } finally {
      this.isSwitchingMode = false;
    }
  }

  destroy() {
    this.isInitialized = false;
  }

  // ── Derived state ──

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
    return (!selfTestBlocking || this.isProcessRunning) && !this.isConnecting && !this.isDisconnecting && !this.isConnected;
  }

  get canDisconnect(): boolean {
    if (this.isInitializing) return false;
    return !this.isConnecting && !this.isDisconnecting && this.isConnected;
  }

  get canStartCore(): boolean {
    if (this.isInitializing) return false;
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return !selfTestBlocking && !this.isCoreBusy && !this.isConnecting && !this.isDisconnecting && !this.isProcessRunning;
  }

  get canStopCore(): boolean {
    return !this.isCoreBusy && !this.isConnecting && !this.isDisconnecting && this.isManagedProcessRunning;
  }

  get canEnableSystemProxy(): boolean {
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return (!selfTestBlocking || this.isProcessRunning) && !this.isSwitchingSystemProxy && !this.isConnecting && !this.isDisconnecting && !this.isSystemProxyEnabled;
  }

  get canDisableSystemProxy(): boolean {
    return !this.isSwitchingSystemProxy && !this.isConnecting && !this.isDisconnecting && this.isSystemProxyEnabled;
  }

  get canEnableTun(): boolean {
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return (!selfTestBlocking || this.isProcessRunning) && !this.isSwitchingTun && !this.isConnecting && !this.isDisconnecting && !this.isTunEnabled;
  }

  get canDisableTun(): boolean {
    return !this.isSwitchingTun && !this.isConnecting && !this.isDisconnecting && this.isTunEnabled;
  }

  get blockingIssues(): string[] {
    return this.selfTest?.blockingIssues ?? [];
  }
}

export const guiState = new GuiStateStore();
