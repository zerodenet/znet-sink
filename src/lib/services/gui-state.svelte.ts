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
  guiSelectPolicy,
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
} from '$lib/types/gui-api';
import type { GuiManagedTunStatus } from '$lib/types/tun';
import type { CommandResult, CommandOptions } from './command-result';
import { RuntimeStatusObserver } from './runtime-status-observer';

const NETWORK_PROBE_INTERVAL_MS = 5 * 60_000;

class GuiStateStore {
  selfTest = $state<SelfTestSnapshot | null>(null);
  connection = $state<ConnectionStatus | null>(null);
  connectionUpdatedAt = $state(0);
  connectionError = $state<string | null>(null);
  selfTestUpdatedAt = $state(0);
  proxyMode = $state<ProxyModeStatus | null>(null);
  coreOverview = $state<CoreOverview | null>(null);
  policyGroups = $state<PolicyGroup[]>([]);
  policyGroupsUpdatedAt = $state(0);
  policyGroupsError = $state<string | null>(null);
  tunStatus = $state<GuiManagedTunStatus | null>(null);
  tunStatusError = $state<string | null>(null);
  private savedTunEnabled = $state<boolean | undefined>(undefined);
  private tunStatusRefreshGate = createLatestRequestGate();
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
  isSelectingPolicy = $state(false);

  private isInitialized = false;
  private lastStatusTick = -1;
  private networkProbeTimer: ReturnType<typeof setInterval> | null = null;
  private networkProbePending = false;
  private internetSharingWarningShown = false;
  private configNodesRefreshGate = createLatestRequestGate();
  private configPolicyGroupsRefreshGate = createLatestRequestGate();
  private policyGroupsRefreshGate = createLatestRequestGate();
  private connectionRefreshGate = createLatestRequestGate();
  private proxyModeRefreshGate = createLatestRequestGate();
  private runtimeStatusObserver = new RuntimeStatusObserver(async () => {
    // Commands own their readback. Background observation resumes afterwards;
    // do not gate on an observed process state, which may itself be stale.
    if (this.isStartingCore || this.isStoppingCore || this.isConnecting || this.isDisconnecting
      || this.isSwitchingSystemProxy || this.isSwitchingTun || this.isSwitchingMode || this.isSelectingPolicy) return;
    await Promise.allSettled([
      this.refreshConnectionStatus(), this.refreshProxyMode(),
      this.refreshPolicyGroups(), this.refreshTunStatus(),
    ]);
  });

  async initialize() {
    if (this.isInitialized) return;
    this.isInitialized = true;
    this.isInitializing = true;

    this.startPeriodicNetworkProbe();
    void this.probeNetwork();
    await this.refreshAll();
    if (!this.isInitialized) return;
    this.runtimeStatusObserver.start();

    // The first authoritative snapshot is complete. UI action guards may now
    // be evaluated normally, including the mode-specific auto-connect below.
    this.isInitializing = false;

    try {
      const appConfig = await getAppConfig();
      if (appConfig.core.autoConnect) {
        await this.autoConnectForMode(
          appConfig.ui.uiMode === 'lite' ? 'lite' : 'pro',
          appConfig.tun.enabled,
        );
      }
    } catch {
      // Configuration errors must not prevent the rest of the UI from loading.
    }
  }

  private async autoConnectForMode(mode: 'lite' | 'pro', desiredTunEnabled?: boolean) {
    if (!this.connection?.coreAvailable) {
      // Kernel startup is asynchronous in Tauri. Give it one short retry, but
      // never make autoConnect itself responsible for starting/stopping Zero.
      await new Promise((resolve) => setTimeout(resolve, 1200));
      await Promise.allSettled([this.refreshConnectionStatus(), this.refreshTunStatus()]);
    }
    if (!this.connection?.coreAvailable) return;

    if (mode === 'lite') {
      // A profile-owned runtime.tun remains authoritative, but Lite still owns
      // the system-proxy side of its combined capture session. Explicit local
      // OFF must survive restarts when the profile itself does not own TUN.
      const profileOwnsTun = this.tunStatus?.configSource === 'profile';
      if (!profileOwnsTun && desiredTunEnabled === false) return;
      if (!this.isConnected) await this.connect();
      return;
    }

    if (this.connection.systemProxyEnabled === true) return;
    this.isConnecting = true;
    try {
      this.acceptConnectionCommand(await tracedOperation('proxy', 'connection.auto_connect', () => guiConnect()));
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
      this.selfTestUpdatedAt = Date.now();
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
    const request = this.connectionRefreshGate.begin();
    try {
      const connection = await getGuiConnectionStatus();
      if (!this.connectionRefreshGate.canApply(request)) return;
      this.connection = connection;
      this.connectionUpdatedAt = Date.now();
      this.connectionError = null;
      this.syncTrayStatus();
    } catch (error) {
      if (!this.connectionRefreshGate.canApply(request)) return;
      this.connectionError = this.errorMessage(error);
      // Preserve the last trusted ownership snapshot through a transient IPC
      // failure instead of making PID/proxy state flicker.
    }
  }

  async refreshProxyMode() {
    const request = this.proxyModeRefreshGate.begin();
    try {
      const mode = await getGuiProxyModeStatus();
      if (this.proxyModeRefreshGate.canApply(request)) this.proxyMode = mode;
    } catch {
      if (this.proxyModeRefreshGate.canApply(request)) this.proxyMode = null;
    }
  }

  private acceptConnectionCommand(connection: ConnectionStatus) {
    // A command acknowledgement supersedes reads started before that command.
    this.connectionRefreshGate.reset();
    this.connection = connection;
    this.connectionUpdatedAt = Date.now();
    this.connectionError = null;
    this.syncTrayStatus();
    return connection;
  }

  private confirmTunCommand(status: GuiManagedTunStatus) {
    this.tunStatusRefreshGate.reset();
    this.savedTunEnabled = status.desiredEnabled;
    this.tunStatusError = null;
    return status;
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
        this.policyGroupsUpdatedAt = Date.now();
        this.policyGroupsError = null;
        return true;
      }
    } catch (e: any) {
      if (this.policyGroupsRefreshGate.canApply(request)) this.policyGroupsError = this.errorMessage(e);
      console.warn('[gui-state] policy groups failed:', this.errorMessage(e));
    }
    return false;
  }

  async refreshNodeStateAfterConfigChange() {
    this.policyGroupsUpdatedAt = 0;
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
    const request = this.tunStatusRefreshGate.begin();
    try {
      const status = await getGuiTunStatus();
      if (!this.tunStatusRefreshGate.canApply(request)) return;
      this.tunStatus = status;
      this.savedTunEnabled = status.desiredEnabled;
      this.tunStatusError = null;
      this.syncTrayStatus();
    } catch (error) {
      if (!this.tunStatusRefreshGate.canApply(request)) return;
      this.tunStatusError = this.errorMessage(error);
      // Preserve observations but expose their uncertainty. Read the saved
      // intent separately so users can cancel ON while status is unavailable.
      const config = await getAppConfig().catch(() => null);
      if (this.tunStatusRefreshGate.canApply(request) && config) {
        this.savedTunEnabled = config.tun.enabled;
      }
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
    void trayUpdateStatus(
      this.isProcessRunning,
      this.isSystemProxyEnabled,
      this.isTunEnabled,
    ).catch(() => {});
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

  /** Compact-mode power lifecycle: system proxy + Zero TUN; Zero process stays alive. */
  async connect() {
    if (!this.canConnect) return;
    this.isConnecting = true;
    this.isSwitchingTun = true;
    let systemProxyStarted = false;
    let tunStarted = false;
    try {
      if (this.connection?.systemProxyEnabled !== true) {
        const connection = this.acceptConnectionCommand(await tracedOperation('proxy', 'lite.system_proxy.enable', () => guiConnect()));
        systemProxyStarted = connection.systemProxyEnabled === true;
        if (!systemProxyStarted) throw new Error('系统代理未进入已开启状态');
      }

      if (!this.isTunEnabled) {
        this.tunStatus = this.confirmTunCommand(await tracedOperation('proxy', 'tun.enable', () => enableGuiTun()));
        tunStarted = this.tunStatus.enabled;
        if (!tunStarted) throw new Error('Zero 未确认 TUN 已启动');
      }

      await this.refreshRuntimeState();
      if (this.connection?.systemProxyEnabled !== true || !this.isTunEnabled) {
        throw new Error('简约模式要求系统代理与 TUN 同时开启');
      }
      this.syncTrayStatus();
      toastSuccess('代理已开启');
      await this.refreshSelfTest();
    } catch (e: any) {
      if (tunStarted) {
        try {
          this.tunStatus = this.confirmTunCommand(await disableGuiTun());
        } catch {
          // Preserve the primary connection failure; refresh below exposes the
          // remaining runtime state if rollback itself fails.
        }
      }
      if (systemProxyStarted) {
        try {
          this.acceptConnectionCommand(await guiDisconnect());
        } catch {
          // Preserve the primary connection failure.
        }
      }
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
      if (this.isTunSwitchOn) {
        this.tunStatus = this.confirmTunCommand(await tracedOperation('proxy', 'tun.disable', () => disableGuiTun()));
        if (this.tunStatus.enabled) throw new Error('Zero 未确认 TUN 已关闭');
      }
      if (this.connection?.systemProxyEnabled === true) {
        this.acceptConnectionCommand(await tracedOperation('proxy', 'lite.system_proxy.disable', () => guiDisconnect()));
      }
      this.syncTrayStatus();
      toastSuccess('代理已关闭，内核保持运行');
      await this.refreshPolicyPanels();
    } catch (e: any) {
      toastError(`断开失败: ${this.errorMessage(e)}`);
      await Promise.allSettled([this.refreshTunStatus(), this.refreshConnectionStatus()]);
    } finally {
      this.isSwitchingTun = false;
      this.isDisconnecting = false;
    }
  }

  async startCore() {
    if (!this.canStartCore) return;
    this.isStartingCore = true;
    this.tunStatusRefreshGate.reset();
    try {
      const result = await tracedOperation('kernel', 'kernel.start', () => startCoreProcess());
      this.tunStatusRefreshGate.reset();
      if (result.tunRestoreError) toastWarning(`内核已启动，但 TUN 恢复失败：${result.tunRestoreError.message}`);
      else toastSuccess('内核监听已启动');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`启动内核失败: ${this.errorMessage(e)}`);
      this.tunStatusRefreshGate.reset();
      await this.refreshRuntimeState();
    } finally {
      this.isStartingCore = false;
    }
  }

  invalidateTunObservation() {
    this.connectionRefreshGate.reset();
    this.proxyModeRefreshGate.reset();
    this.tunStatusRefreshGate.reset();
    this.tunStatus = null;
    this.tunStatusError = '正在重启内核，等待确认 TUN 状态';
  }

  async restartCore() {
    if (!this.canRestartCore) return;
    this.isStoppingCore = true;
    // Runtime observations belong to the old Core instance. Drop the TUN
    // projection before the process generation changes, then rebuild it from
    // the new Core after restart.
    this.invalidateTunObservation();
    try {
      const result = await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess());
      this.tunStatusRefreshGate.reset();
      if (result.tunRestoreError) toastWarning(`内核已重启，但 TUN 恢复失败：${result.tunRestoreError.message}`);
      else toastSuccess('内核已重启');
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
    } catch (e: any) {
      toastError(`重启内核失败: ${this.errorMessage(e)}`);
      this.tunStatusRefreshGate.reset();
      await this.refreshRuntimeState();
    } finally {
      this.isStoppingCore = false;
    }
  }

  private reportCommandConfirmation(confirmed: boolean, success: string, unconfirmed: string, notify = true): CommandResult {
    if (confirmed) { if (notify) toastSuccess(success); return { ok: true }; }
    if (notify) toastWarning(unconfirmed);
    return { ok: false, message: unconfirmed };
  }

  async enableSystemProxy({ notify = true }: CommandOptions = {}): Promise<CommandResult> {
    if (!this.canEnableSystemProxy) return { ok: false, message: '当前无法开启系统代理，请检查内核与配置状态' };
    this.isSwitchingSystemProxy = true;
    try {
      await tracedOperation('proxy', 'system_proxy.enable', () => enableSystemProxyCommand());
      await this.refreshRuntimeState();
      await this.refreshSelfTest();
      return this.reportCommandConfirmation(!this.connectionError && this.isSystemProxyEnabled, '系统代理已开启', '开启请求已完成，但尚未确认系统代理状态，请重新检查', notify);
    } catch (e: any) {
      if (notify) toastError(`开启系统代理失败: ${this.errorMessage(e)}`);
      await this.refreshRuntimeState();
      return { ok: false, message: this.errorMessage(e) };
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async disableSystemProxy({ notify = true }: CommandOptions = {}): Promise<CommandResult> {
    if (!this.canDisableSystemProxy) return { ok: false, message: '系统代理正在切换，请稍后重试' };
    this.isSwitchingSystemProxy = true;
    try {
      await tracedOperation('proxy', 'system_proxy.disable', () => disableSystemProxyCommand());
      await this.refreshConnectionStatus();
      return this.reportCommandConfirmation(!this.connectionError && !this.isSystemProxyEnabled, '系统代理已关闭', '关闭请求已完成，但尚未确认系统代理状态，请重新检查', notify);
    } catch (e: any) {
      if (notify) toastError(`关闭系统代理失败: ${this.errorMessage(e)}`);
      await this.refreshConnectionStatus();
      return { ok: false, message: this.errorMessage(e) };
    } finally {
      this.isSwitchingSystemProxy = false;
    }
  }

  async toggleSystemProxy({ notify = true }: CommandOptions = {}) {
    if (this.connection?.systemProxyEnabled === true) {
      return this.disableSystemProxy({ notify });
    } else {
      return this.enableSystemProxy({ notify });
    }
  }

  async enableTun({ notify = true }: CommandOptions = {}): Promise<CommandResult> {
    if (!this.canEnableTun) return { ok: false, message: '当前无法开启 TUN，请检查内核、配置与权限' };
    this.isSwitchingTun = true;
    this.tunStatusRefreshGate.reset();
    try {
      const status = await enableGuiTun();
      this.tunStatusRefreshGate.reset();
      this.tunStatus = status;
      await this.refreshRuntimeState();
      return this.reportCommandConfirmation(!this.tunStatusError && this.isTunEnabled && this.tunStatus?.healthy === true, 'TUN 已开启', this.tunStatus?.lastError || '开启请求已完成，但尚未确认 TUN 健康接管，请重新检查', notify);
    } catch (e: any) {
      if (notify) toastError(`开启 TUN 失败: ${this.errorMessage(e)}`);
      this.tunStatusRefreshGate.reset();
      await this.refreshTunStatus();
      await this.refreshConnectionStatus();
      return { ok: false, message: this.errorMessage(e) };
    } finally {
      this.isSwitchingTun = false;
    }
  }

  async disableTun({ notify = true }: CommandOptions = {}): Promise<CommandResult> {
    if (!this.canDisableTun) return { ok: false, message: '当前无法关闭 TUN，请等待正在进行的操作完成' };
    this.isSwitchingTun = true;
    this.tunStatusRefreshGate.reset();
    try {
      const status = await disableGuiTun();
      this.tunStatusRefreshGate.reset();
      this.tunStatus = status;
      await this.refreshTunStatus();
      return this.reportCommandConfirmation(!this.tunStatusError && !this.isTunSwitchOn, 'TUN 已关闭', '关闭请求已完成，但尚未确认 TUN 与自动恢复设置，请重新检查', notify);
    } catch (e: any) {
      if (notify) toastError(`关闭 TUN 失败: ${this.errorMessage(e)}`);
      this.tunStatusRefreshGate.reset();
      await this.refreshTunStatus();
      return { ok: false, message: this.errorMessage(e) };
    } finally {
      this.isSwitchingTun = false;
    }
  }

  async toggleTun({ notify = true }: CommandOptions = {}) {
    if (this.isTunSwitchOn) return this.disableTun({ notify });
    else return this.enableTun({ notify });
  }

  async setProxyMode(mode: ProxyMode, { notify = true }: CommandOptions = {}): Promise<CommandResult> {
    if (this.isSwitchingMode || this.isCoreBusy || !this.connection?.coreAvailable || !this.proxyMode?.availableModes.includes(mode)) {
      return { ok: false, message: '当前无法切换代理模式，请检查内核状态与支持的模式' };
    }
    this.isSwitchingMode = true;
    try {
      const confirmed = await guiSetProxyMode(mode);
      this.proxyModeRefreshGate.reset();
      this.proxyMode = confirmed;
      await this.refreshModeState();
      return this.proxyMode?.currentMode === mode
        ? { ok: true }
        : { ok: false, message: '模式请求已提交，但尚未确认生效，请重新检查' };
    } catch (e: any) {
      if (notify) toastError(`切换代理模式失败: ${this.errorMessage(e)}`);
      await this.refreshModeState();
      return { ok: false, message: this.errorMessage(e) };
    } finally {
      this.isSwitchingMode = false;
    }
  }

  async selectPolicy(policyTag: string, targetTag: string): Promise<CommandResult> {
    const group = this.policyGroups.find((item) => item.name === policyTag);
    if (this.isSelectingPolicy || this.isCoreBusy || !this.connection?.coreAvailable) return { ok: false, message: '内核未就绪或正在切换策略' };
    if (group?.kind?.toLowerCase() !== 'selector' || !group.outbounds.some((item) => item.tag === targetTag)) {
      return { ok: false, message: '只能选择手动策略组中的直接成员' };
    }
    this.isSelectingPolicy = true;
    try {
      const result = await guiSelectPolicy(policyTag, targetTag);
      if (!result.accepted) return { ok: false, message: result.message || '内核未接受此选择' };
      const refreshed = await this.refreshPolicyGroups();
      return refreshed && this.policyGroups.find((item) => item.name === policyTag)?.selected === targetTag
        ? { ok: true }
        : { ok: false, message: '选择已提交，但尚未确认实际出口，请重新检查' };
    } catch (error) {
      await this.refreshPolicyGroups();
      return { ok: false, message: this.errorMessage(error) };
    } finally { this.isSelectingPolicy = false; }
  }

  destroy() {
    this.isInitialized = false;
    this.runtimeStatusObserver.stop();
    this.connectionRefreshGate.reset();
    this.proxyModeRefreshGate.reset();
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

  /** Lite power is fully on only when both client capture layers are active. */
  get isConnected(): boolean {
    return this.isTunEnabled && this.connection?.systemProxyEnabled === true;
  }

  /** Actual GUI-managed operating-system proxy ownership. */
  get isSystemProxyEnabled(): boolean {
    return this.connection?.systemProxyEnabled === true;
  }

  get isTunEnabled(): boolean {
    return this.tunStatus?.enabled === true;
  }

  get isTunDesiredEnabled(): boolean {
    return (this.savedTunEnabled ?? this.tunStatus?.desiredEnabled) === true;
  }

  get isTunSwitchOn(): boolean {
    return this.isTunEnabled || this.isTunDesiredEnabled;
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
      && !this.isConnected;
  }

  get canDisconnect(): boolean {
    if (this.isInitializing) return false;
    return !this.isConnecting
      && !this.isDisconnecting
      && !this.isSwitchingTun
      && (this.isTunSwitchOn || this.connection?.systemProxyEnabled === true);
  }

  get canStartCore(): boolean {
    if (this.isInitializing) return false;
    // A missing proxy profile must not prevent the user from starting the
    // management-only kernel. System proxy/TUN actions keep their stricter
    // profile guards; only launch-critical self-test failures block startup.
    const launchBlocking = this.selfTest?.checks.some((check) =>
      check.status === 'fail'
      && check.key !== 'activeProxyConfig'
      && check.key !== 'activeProxyContent'
    ) ?? false;
    return !launchBlocking
      && !this.isCoreBusy
      && !this.isConnecting
      && !this.isDisconnecting
      && !this.isProcessRunning;
  }

  get canRestartCore(): boolean {
    return !this.isCoreBusy && !this.isSwitchingTun && !this.isConnecting && !this.isDisconnecting && this.isManagedProcessRunning;
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
      && !this.isCoreBusy
      && !this.isSwitchingTun
      && !this.isConnecting
      && !this.isDisconnecting
      && !this.isTunEnabled;
  }

  get canDisableTun(): boolean {
    return !this.isCoreBusy && !this.isSwitchingTun && !this.isConnecting && !this.isDisconnecting && this.isTunSwitchOn;
  }

  get blockingIssues(): string[] {
    return this.selfTest?.blockingIssues ?? [];
  }
}

export const guiState = new GuiStateStore();
