import {
  getGuiSelfTestSnapshot,
  getGuiConnectionStatus,
  guiConnect,
  guiDisconnect,
  startCoreProcess,
  stopCoreProcess,
  enableSystemProxy as enableSystemProxyCommand,
  disableSystemProxy as disableSystemProxyCommand,
  getGuiTunStatus,
  enableGuiTun,
  disableGuiTun,
  getGuiProxyModeStatus,
  guiSetProxyMode,
  getGuiCoreOverview,
  getGuiPolicyGroups,
} from './core';
import { error as toastError, success as toastSuccess } from './toast.svelte';
import type {
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

    await this.refreshAll();
  }

  async refreshAll() {
    await Promise.allSettled([
      this.refreshSelfTest(),
      this.refreshConnectionStatus(),
      this.refreshProxyMode(),
      this.refreshCoreOverview(),
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

  async refreshPolicyGroups() {
    try {
      this.policyGroups = await getGuiPolicyGroups();
    } catch {
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

  async connect() {
    this.isConnecting = true;
    try {
      this.connection = await guiConnect();
      toastSuccess('系统代理已开启，服务已生效');
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
      toastSuccess('系统代理已关闭，内核已停止');
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
      toastSuccess('内核已停止，系统代理已关闭');
      await this.refreshRuntimeState();
    } catch (e: any) {
      toastError(`停止内核失败: ${this.errorMessage(e)}`);
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
      await this.refreshConnectionStatus();
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
    const selfTestBlocking = this.selfTest !== null && !this.selfTest.ready;
    return (!selfTestBlocking || this.isProcessRunning) && !this.isConnecting && !this.isDisconnecting && !this.isConnected;
  }

  get canDisconnect(): boolean {
    return !this.isConnecting && !this.isDisconnecting && this.isConnected;
  }

  get canStartCore(): boolean {
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
