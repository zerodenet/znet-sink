import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { startGuiEvents, stopGuiEvents, appendLog, getCoreStats, getCoreRuntime } from '$lib/services/core';
import { overviewData } from '$lib/services/overview-data.svelte';
import { guiState } from '$lib/services/gui-state.svelte';
import { delayHistory } from '$lib/services/delay-history.svelte';
import {
  buildPolicyProbeHistoryUpdates,
  projectSelectedGroupHistoryUpdates,
} from '$lib/services/policy-probe-history';
import { EventLifecycleQueue } from '$lib/services/event-lifecycle';
import { warning as showWarningToast } from '$lib/services/toast.svelte';
import type { CoreEventStatus, GuiEventPayload, TunStatusEvent, StackStatusEvent } from '$lib/types/core';
import type { GuiConnectionItem, PolicyProbeCompletedEvent } from '$lib/types/gui-api';

const EVENT_NAME = 'gui:event';
const STATUS_NAME = 'gui:event-status';
const HOST_NETWORK_CHANGED_EVENT = 'host-network:changed';

export class PolicyProbeTimeoutError extends Error {
  readonly code = 'policy_probe_timeout';

  constructor(readonly policyTag: string) {
    super(`policy probe timed out: ${policyTag}`);
    this.name = 'PolicyProbeTimeoutError';
  }
}

// ── Exported types ──

export type ConnectionDelta =
  | { type: 'started'; connection: GuiConnectionItem }
  | { type: 'updated'; connection: GuiConnectionItem }
  | { type: 'completed'; connection: GuiConnectionItem }
  | { type: 'snapshot'; connections: GuiConnectionItem[] };

export interface CoreWarning {
  code?: string;
  message: string;
  timestamp: number;
}

class CoreEventsService {
  isSubscribed = $state(false);
  status = $state<'idle' | 'subscribed' | 'reconnecting' | 'offline' | 'error' | 'disconnected'>('idle');
  lastError = $state<string | null>(null);
  connectionTick = $state(0);
  activeConnections = $state<GuiConnectionItem[]>([]);
  connectionHistory = $state<GuiConnectionItem[]>([]);

  // 日志刷新计数器（LogPanel 响应）
  logTick = $state(0);

  // 核心状态刷新计数器（CoreStatusCard 响应）
  statusTick = $state(0);

  // 内核警告
  lastWarning = $state<CoreWarning | null>(null);
  warnings = $state<CoreWarning[]>([]);

  // v0.0.5+: TUN 虚拟网卡状态
  tunState = $state<'idle' | 'started' | 'stopped' | 'error'>('idle');
  tunStateMessage = $state<string | null>(null);

  // v0.0.5+: 内核网络栈状态（不是 GUI 系统代理开关）
  stackState = $state<'idle' | 'started' | 'stopped' | 'degraded'>('idle');
  stackMode = $state<string | null>(null);

  // 连接增量事件
  private _deltaSeq = $state(0);
  private _pendingDeltas: ConnectionDelta[] = [];

  get deltaSeq() { return this._deltaSeq; }

  private _unlistenEvent: UnlistenFn | null = null;
  private _unlistenStatus: UnlistenFn | null = null;
  private _unlistenProcess: UnlistenFn | null = null;
  private _unlistenHostNetwork: UnlistenFn | null = null;
  private _activeGeneration: number | null = null;
  private _lifecycle = new EventLifecycleQueue();

  private _stopped = true;
  private _policyProbeWaiters = new Map<
    string,
    Set<(event: PolicyProbeCompletedEvent) => void>
  >();

  /** Register before sending policies.probe so a fast completion event
   * cannot race past the UI lifecycle. */
  waitForPolicyProbe(
    policyTag: string,
    options: { timeoutMs?: number; trigger?: string } = {},
  ): Promise<PolicyProbeCompletedEvent> {
    return new Promise((resolve, reject) => {
      const waiters = this._policyProbeWaiters.get(policyTag) ?? new Set();
      let timer: ReturnType<typeof setTimeout>;
      const complete = (event: PolicyProbeCompletedEvent) => {
        // A scheduled probe for the same policy may finish while a manual
        // request is in flight. It should still update global state, but must
        // not end the clicked card's lifecycle. Older kernels that omit the
        // trigger remain compatible.
        if (options.trigger && event.trigger && event.trigger !== options.trigger) return;
        clearTimeout(timer);
        waiters.delete(complete);
        if (waiters.size === 0) this._policyProbeWaiters.delete(policyTag);
        resolve(event);
      };
      waiters.add(complete);
      this._policyProbeWaiters.set(policyTag, waiters);
      timer = setTimeout(() => {
        waiters.delete(complete);
        if (waiters.size === 0) this._policyProbeWaiters.delete(policyTag);
        reject(new PolicyProbeTimeoutError(policyTag));
      }, options.timeoutMs ?? 60_000);
    });
  }

  start(events?: string[]): Promise<void> {
    return this._lifecycle.enqueue(() => this._start(events));
  }

  private async _start(events?: string[]) {
    // The app lifecycle owns this stream. Repeated calls must not rotate the
    // backend generation because an older async start can otherwise overwrite
    // the active generation selected by a newer call.
    if (
      !this._stopped
      && this._activeGeneration !== null
      && this._unlistenEvent
      && this._unlistenStatus
      && this._unlistenProcess
      && this._unlistenHostNetwork
    ) {
      return;
    }
    this._stopped = false;

    try {
      // Listen before starting subscription so we don't miss status events.
      // Listener registration is part of the recoverable kernel boundary too:
      // a closed Tauri channel must become UI error state, not an unhandled
      // rejection that can tear down the page lifecycle.
      if (!this._unlistenEvent) {
        this._unlistenEvent = await listen<GuiEventPayload>(EVENT_NAME, (event) => {
          this._routeEvent(event.payload);
        });
      }
      if (!this._unlistenStatus) {
        this._unlistenStatus = await listen<CoreEventStatus>(STATUS_NAME, (event) => {
          this._handleStatus(event.payload);
        });
      }
      if (!this._unlistenProcess) {
        this._unlistenProcess = await listen<{ reason: string; code: number | null; message: string }>('core:process-exited', (event) => {
          this._handleProcessExited(event.payload);
        });
      }
      if (!this._unlistenHostNetwork) {
        this._unlistenHostNetwork = await listen<{ reason: string; occurredAtUnixMs: number }>(
          HOST_NETWORK_CHANGED_EVENT,
          () => {
            void guiState.probeNetwork();
            void guiState.refreshSelfTest();
          },
        );
      }

      const sub = await startGuiEvents(events);
      this._activeGeneration = sub.generation;
    } catch (e) {
      this.status = 'error';
      this.lastError = String(e);
    }
  }

  stop(): Promise<void> {
    return this._lifecycle.enqueue(() => this._stop());
  }

  private async _stop() {
    this._stopped = true;
    try {
      await stopGuiEvents();
    } catch {
      // Listener teardown is still required when the backend is unavailable.
    }
    this._activeGeneration = null;
    this.isSubscribed = false;
    this.status = 'idle';
    this._unlistenEvent?.();
    this._unlistenStatus?.();
    this._unlistenProcess?.();
    this._unlistenHostNetwork?.();
    this._unlistenEvent = null;
    this._unlistenStatus = null;
    this._unlistenProcess = null;
    this._unlistenHostNetwork = null;
    this._pendingDeltas = [];
    this.activeConnections = [];
    this._policyProbeWaiters.clear();
  }

  /** 获取并清空待处理的连接增量事件 */
  drainDeltas(): ConnectionDelta[] {
    const deltas = this._pendingDeltas;
    this._pendingDeltas = [];
    return deltas;
  }

  /** 清除所有警告（用户确认后调用） */
  clearWarnings() {
    this.warnings = [];
  }

  private _handleStatus(status: CoreEventStatus) {
    if (this._stopped) return;
    // The backend can emit the first status before the invoke response carries
    // its generation back to JavaScript. Accept it while start() is in flight;
    // once active, reject stale generations normally.
    if (this._activeGeneration !== null && status.generation !== this._activeGeneration) return;

    switch (status.status) {
      case 'subscribed':
        this.isSubscribed = true;
        this.status = 'subscribed';
        this.lastError = null;
        awaitIgnore(this._applyResyncSnapshot(status.response));
        this.statusTick++;
        break;
      case 'reconnecting':
        this.isSubscribed = false;
        this.status = 'reconnecting';
        overviewData.isLive = false;
        this.statusTick++;
        break;
      case 'offline':
        this.isSubscribed = false;
        this.status = 'offline';
        this.lastError = status.error?.message ?? 'core is not available';
        overviewData.isLive = false;
        this.statusTick++;
        break;
      case 'disconnected':
        this.isSubscribed = false;
        this.status = 'disconnected';
        overviewData.isLive = false;
        this.statusTick++;
        break;
      case 'stopped':
        this.isSubscribed = false;
        this.status = 'idle';
        this.statusTick++;
        break;
      case 'error':
        this.isSubscribed = false;
        this.status = 'error';
        this.lastError = status.error?.message ?? 'unknown error';
        overviewData.isLive = false;
        this.statusTick++;
        break;
    }
  }

  private _routeEvent(payload: GuiEventPayload) {
    const { generation: _gen, event } = payload;
    if (this._stopped) return;
    if (!event || typeof event !== 'object') return;
    if (this._activeGeneration !== null && _gen !== this._activeGeneration) return;

    const eventType = event.eventType;
    const eventPayload = event.payload;
    const data = this._eventData(eventPayload);
    const obj = data && typeof data === 'object'
      ? data as Record<string, unknown>
      : { eventType, sourceEventType: event.sourceEventType };
    const type = typeof obj['type'] === 'string' ? obj['type'] : '';
    const subtype = typeof obj['subtype'] === 'string' ? obj['subtype'] : '';

    if (eventType === 'traffic.sampled') {
      overviewData.applyStatsEvent(obj);
      return;
    }

    if (eventType === 'policy.probeCompleted') {
      const probe = data as PolicyProbeCompletedEvent;
      if (probe?.policyTag && Array.isArray(probe.members)) {
        guiState.applyPolicyProbeCompleted(probe);
        const historyUpdates = projectSelectedGroupHistoryUpdates(
          guiState.policyGroups,
          buildPolicyProbeHistoryUpdates(probe),
        );
        for (const update of historyUpdates) {
          delayHistory.record(
            update.tag,
            update.delayMs,
            update.reachable,
            update.at,
            update.selectedTag,
          );
        }
        const waiters = this._policyProbeWaiters.get(probe.policyTag);
        if (waiters) {
          for (const complete of [...waiters]) complete(probe);
        }
      }
      awaitIgnore(overviewData.refreshPolicyNodes());
      this.statusTick++;
      return;
    }

    if (eventType === 'policy.selected') {
      awaitIgnore(overviewData.refreshPolicyNodes());
      this.statusTick++;
      return;
    }

    if (eventType === 'core.configChanged') {
      awaitIgnore(this._fetchInitialState());
      this.statusTick++;
      return;
    }

    // ── 内核状态变化（引擎启动/停止）──
    if (eventType === 'core.statusChanged') {
      this._handleCoreStatus(data);
      return;
    }

    // ── 内核警告通知 ──
    if (eventType === 'core.warning') {
      this._handleCoreWarning(data);
      return;
    }

    // ── v0.0.5+: TUN 虚拟网卡状态变化 ──
    if (eventType === 'tun.statusChanged') {
      this._handleTunStatus(data);
      return;
    }

    if (eventType === 'tun.error') {
      this._handleTunError(data);
      return;
    }

    // ── v0.0.5+: 内核网络栈状态变化（不是 GUI 系统代理开关）──
    if (eventType === 'stack.statusChanged') {
      this._handleStackStatus(data);
      return;
    }

    // ── 连接实时事件（增量更新）──
    if (eventType === 'connection.snapshot') {
      const connections = Array.isArray(data)
        ? data.map((item) => this._parseConnectionEvent(item)).filter((item): item is GuiConnectionItem => item !== null)
        : [];
      this._pushDelta({ type: 'snapshot', connections });
      this.connectionTick++;
      return;
    }

    if (eventType === 'connection.started' || eventType === 'connection.updated') {
      const conn = this._parseConnectionEvent(data);
      if (conn) {
        this._pushDelta({
          type: eventType === 'connection.started' ? 'started' : 'updated',
          connection: conn,
        });
      }
      this.connectionTick++;
      return;
    }

    if (eventType === 'connection.closed') {
      const conn = this._parseConnectionEvent(data);
      if (conn) {
        this._pushDelta({ type: 'completed', connection: conn });
      }
      this.connectionTick++;
      return;
    }

    // ── IPC 客户端连接/断开（诊断用，不驱动 UI）──
    if (eventType === 'core.ipcStatus') {
      this._handleIpcStatus(data);
      return;
    }

    // ── 未知事件 → 记录日志用于调试（但不要污染 UI）──
    if (eventType === 'core.unknownEvent') {
      this._logUnknownEvent(data, event.sourceEventType);
      return;
    }

    // ══ 以下为兜底路由：靠字段特征匹配原始内核事件（旧兼容路径）══
    // 新事件类型应在上方添加显式 handler，不应依赖此段匹配

    // Stats events
    if (
      type === 'stats' ||
      subtype === 'stats' ||
      this._hasAnyKey(obj, ['uploadSpeed', 'downloadSpeed', 'upload_speed', 'download_speed', 'txSpeed', 'rxSpeed', 'connections', 'connectionCount'])
    ) {
      overviewData.applyStatsEvent(obj);
      return;
    }

    // Runtime / node events
    if (
      type === 'runtime' ||
      type === 'config' ||
      this._hasAnyKey(obj, ['proxies', 'outbounds', 'nodes'])
    ) {
      overviewData.applyRuntimeEvent(obj);
      return;
    }

    // Log events
    if (
      type === 'log' ||
      subtype === 'log' ||
      (typeof obj['level'] === 'string' && typeof obj['message'] === 'string')
    ) {
      const level = (typeof obj['level'] === 'string' ? obj['level'] : 'info') as 'trace' | 'debug' | 'info' | 'warn' | 'error';
      const message = typeof obj['message'] === 'string' ? obj['message'] : JSON.stringify(obj);
      appendLog({ source: 'core', level, message, fields: obj }).catch(() => {});
      this.logTick++;
      return;
    }

    // Connection / flow events — signal live change so listeners can refresh
    if (
      type === 'flow' ||
      type === 'connection' ||
      typeof obj['flow_id'] === 'string' ||
      typeof obj['flowId'] === 'string'
    ) {
      this.connectionTick++;
      return;
    }
  }

  // ── 内核警告 ──

  private _handleCoreWarning(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : {};
    const code = typeof obj['code'] === 'string' ? obj['code'] : undefined;
    const message = typeof obj['message'] === 'string' ? obj['message'] : '内核引擎产生警告';

    const w: CoreWarning = { code, message, timestamp: Date.now() };
    this.lastWarning = w;
    this.warnings = [w, ...this.warnings].slice(0, 50);
    // engine.warning includes every tracing WARN/ERROR emitted by Zero. Those
    // high-volume operational records already enter the persistent CORE log
    // through the stderr pump; treating each one as a user notification makes
    // transient relay failures obscure the controls without offering an
    // actionable response.
  }

  private _handleCoreStatus(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : null;
    if (!obj) return;

    const healthy = typeof obj['healthy'] === 'boolean' ? obj['healthy'] : false;
    overviewData.isLive = healthy;

    // 引擎恢复 → 触发状态更新，下游组件可依此对账
    if (healthy) {
      awaitIgnore(this._fetchInitialState());
    }
    this.statusTick++;
  }

  // ── v0.0.5+: TUN 虚拟网卡事件 ──

  private _handleTunStatus(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : {};
    const state = (typeof obj['state'] === 'string' ? obj['state'] : 'idle') as TunStatusEvent['state'];
    this.tunState = state;
    this.tunStateMessage = typeof obj['message'] === 'string' ? obj['message'] : null;

    if (state === 'error') {
      const msg = this.tunStateMessage ?? 'TUN interface error';
      showWarningToast(`TUN: ${msg}`, 5000);
    }
  }

  private _handleTunError(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : {};
    this.tunState = 'error';
    this.tunStateMessage = typeof obj['message'] === 'string' ? obj['message'] : 'TUN interface error';
    showWarningToast(`TUN 错误: ${this.tunStateMessage}`, 6000);
  }

  // ── 内核进程退出（崩溃监控线程通知）──
  private _handleProcessExited(payload: { reason: string; code: number | null; message: string }) {
    this.statusTick++;
    if (payload.reason === 'crashed') {
      showWarningToast(`内核崩溃 (code=${payload.code ?? '?'})`, 8000);
    }
  }

  // ── v0.0.5+: 内核网络栈状态事件（不是 GUI 系统代理开关）──

  private _handleStackStatus(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : {};
    const state = (typeof obj['state'] === 'string' ? obj['state'] : 'idle') as StackStatusEvent['state'];
    this.stackState = state;
    this.stackMode = typeof obj['mode'] === 'string' ? obj['mode'] : null;

    if (state === 'degraded') {
      const msg = typeof obj['message'] === 'string' ? obj['message'] : 'stack degraded';
      showWarningToast(`网络栈降级: ${msg}`, 5000);
    }
  }

  // ── 连接增量 ──

  private _pushDelta(delta: ConnectionDelta) {
    this._projectConnectionDelta(delta);
    this._pendingDeltas.push(delta);
    if (this._pendingDeltas.length > 2_000) {
      this._pendingDeltas.splice(0, this._pendingDeltas.length - 2_000);
    }
    this._deltaSeq++;
  }

  private _projectConnectionDelta(delta: ConnectionDelta) {
    if (delta.type === 'snapshot') {
      this.activeConnections = delta.connections.slice(0, 500);
      return;
    }

    if (delta.type === 'completed') {
      const active = this.activeConnections.find((item) => item.flowId === delta.connection.flowId);
      if (active && isOlderRevision(active, delta.connection)) return;
      this.activeConnections = this.activeConnections.filter((item) => item.flowId !== delta.connection.flowId);
      this.connectionHistory = [
        delta.connection,
        ...this.connectionHistory.filter((item) => item.flowId !== delta.connection.flowId),
      ].slice(0, 500);
      return;
    }

    const index = this.activeConnections.findIndex((item) => item.flowId === delta.connection.flowId);
    const current = index >= 0 ? this.activeConnections[index] : undefined;
    if (current && isOlderRevision(current, delta.connection)) return;
    const connection = mergeGuiConnection(current, delta.connection);
    this.activeConnections = [
      connection,
      ...this.activeConnections.filter((item) => item.flowId !== connection.flowId),
    ].slice(0, 500);
  }

  private _parseConnectionEvent(data: unknown): GuiConnectionItem | null {
    if (!data || typeof data !== 'object') return null;
    const o = data as Record<string, unknown>;
    const flowId = typeof o['flowId'] === 'string' ? o['flowId'] : null;
    if (!flowId) return null;

    return {
      flowId,
      revision: typeof o['revision'] === 'number' ? o['revision'] : undefined,
      state: typeof o['state'] === 'string' ? o['state'] : undefined,
      network: typeof o['network'] === 'string' ? o['network'] : 'tcp',
      source: typeof o['source'] === 'string' ? o['source'] : undefined,
      sourceIp: typeof o['sourceIp'] === 'string' ? o['sourceIp'] : undefined,
      sourcePort: typeof o['sourcePort'] === 'number' ? o['sourcePort'] : undefined,
      processId: typeof o['processId'] === 'number' ? o['processId'] : undefined,
      processName: typeof o['processName'] === 'string' ? o['processName'] : undefined,
      processPath: typeof o['processPath'] === 'string' ? o['processPath'] : undefined,
      destination: typeof o['destination'] === 'string' ? o['destination'] : '-',
      targetHost: typeof o['targetHost'] === 'string' ? o['targetHost'] : undefined,
      targetIp: typeof o['targetIp'] === 'string' ? o['targetIp'] : undefined,
      targetPort: typeof o['targetPort'] === 'number' ? o['targetPort'] : undefined,
      sniffedHost: typeof o['sniffedHost'] === 'string' ? o['sniffedHost'] : undefined,
      inboundTag: typeof o['inboundTag'] === 'string' ? o['inboundTag'] : undefined,
      inboundProtocol: typeof o['inboundProtocol'] === 'string' ? o['inboundProtocol'] : undefined,
      outboundTag: typeof o['outboundTag'] === 'string' ? o['outboundTag'] : undefined,
      outboundProtocol: typeof o['outboundProtocol'] === 'string' ? o['outboundProtocol'] : undefined,
      remoteDestination: typeof o['remoteDestination'] === 'string' ? o['remoteDestination'] : undefined,
      policyTag: typeof o['policyTag'] === 'string' ? o['policyTag'] : undefined,
      routeMode: typeof o['routeMode'] === 'string' ? o['routeMode'] : undefined,
      routeAction: typeof o['routeAction'] === 'string' ? o['routeAction'] : undefined,
      matchedRuleIndex: typeof o['matchedRuleIndex'] === 'number' ? o['matchedRuleIndex'] : undefined,
      matchedRule: typeof o['matchedRule'] === 'string' ? o['matchedRule'] : undefined,
      selectionChain: Array.isArray(o['selectionChain'])
        ? o['selectionChain'].filter((item): item is string => typeof item === 'string')
        : [],
      relayChain: Array.isArray(o['relayChain'])
        ? o['relayChain'].filter((item): item is string => typeof item === 'string')
        : [],
      outcome: typeof o['outcome'] === 'string' ? o['outcome'] : undefined,
      closeReason: typeof o['closeReason'] === 'string' ? o['closeReason'] : undefined,
      failureStage: typeof o['failureStage'] === 'string' ? o['failureStage'] : undefined,
      failureCode: typeof o['failureCode'] === 'string' ? o['failureCode'] : undefined,
      failureMessage: typeof o['failureMessage'] === 'string' ? o['failureMessage'] : undefined,
      bytesUp: typeof o['bytesUp'] === 'number' ? o['bytesUp'] : 0,
      bytesDown: typeof o['bytesDown'] === 'number' ? o['bytesDown'] : 0,
      inboundRxBytes: typeof o['inboundRxBytes'] === 'number' ? o['inboundRxBytes'] : undefined,
      inboundTxBytes: typeof o['inboundTxBytes'] === 'number' ? o['inboundTxBytes'] : undefined,
      outboundRxBytes: typeof o['outboundRxBytes'] === 'number' ? o['outboundRxBytes'] : undefined,
      outboundTxBytes: typeof o['outboundTxBytes'] === 'number' ? o['outboundTxBytes'] : undefined,
      throughputUpBps: typeof o['throughputUpBps'] === 'number' ? o['throughputUpBps'] : undefined,
      throughputDownBps: typeof o['throughputDownBps'] === 'number' ? o['throughputDownBps'] : undefined,
      startedAtUnixMs: typeof o['startedAtUnixMs'] === 'number' ? o['startedAtUnixMs'] : undefined,
      lastActivityAtUnixMs: typeof o['lastActivityAtUnixMs'] === 'number' ? o['lastActivityAtUnixMs'] : undefined,
      endedAtUnixMs: typeof o['endedAtUnixMs'] === 'number' ? o['endedAtUnixMs'] : undefined,
      updatedAtUnixMs: typeof o['updatedAtUnixMs'] === 'number' ? o['updatedAtUnixMs'] : undefined,
      durationMs: typeof o['durationMs'] === 'number' ? o['durationMs'] : undefined,
    };
  }

  private _handleIpcStatus(data: unknown) {
    const obj = data && typeof data === 'object' ? data as Record<string, unknown> : {};
    console.debug('[ZNet] ipc status', {
      active: obj['active'],
      pipe: obj['pipe'],
      error: obj['error'],
    });
    // Diagnostic only — not surfaced to user UI
  }

  private _logUnknownEvent(data: unknown, sourceType: string) {
    const summary = data && typeof data === 'object'
      ? JSON.stringify(data).slice(0, 200)
      : String(data ?? 'null').slice(0, 200);
    console.debug('[ZNet] unknown core event', { sourceType, summary });
    // 不写入用户日志面板——这不是用户可操作的信息
  }

  private _hasAnyKey(obj: Record<string, unknown>, keys: string[]): boolean {
    return keys.some((k) => k in obj);
  }

  private _eventData(payload: unknown): unknown {
    if (!payload || typeof payload !== 'object') return payload;
    const obj = payload as Record<string, unknown>;
    return 'data' in obj ? obj['data'] : payload;
  }

  // ── Auto-reconnect with exponential backoff ──

  private async _applyResyncSnapshot(snapshot: unknown) {
    if (!snapshot || typeof snapshot !== 'object') {
      await this._fetchInitialState();
      return;
    }

    const data = snapshot as Record<string, unknown>;
    const stats = data['stats'];
    const runtime = data['runtime'];
    const policies = data['policies'];
    const connectionSnapshot = data['connections'];

    if (stats && typeof stats === 'object') {
      overviewData.applyStatsEvent(stats as Record<string, unknown>);
    }
    if (runtime && typeof runtime === 'object') {
      overviewData.applyRuntimeEvent(runtime as Record<string, unknown>);
    }
    if (policies) {
      overviewData.applyPolicyEvent(policies);
    }
    if (connectionSnapshot && typeof connectionSnapshot === 'object') {
      const items = (connectionSnapshot as Record<string, unknown>)['items'];
      if (Array.isArray(items)) {
        const connections = items
          .map((item) => this._parseConnectionEvent(item))
          .filter((item): item is GuiConnectionItem => item !== null);
        this._pushDelta({ type: 'snapshot', connections });
        this.connectionTick++;
      }
    }

    if (!stats || !runtime || !policies) {
      await this._fetchInitialState();
    }
  }

  

  private async _fetchInitialState() {
    try {
      const [statsResult, runtimeResult] = await Promise.all([
        getCoreStats(),
        getCoreRuntime(),
      ]);
      overviewData.applyStatsEvent(statsResult);
      overviewData.applyRuntimeEvent(runtimeResult);
      await overviewData.refreshPolicyNodes();
    } catch {
      // Best-effort initial fetch
    }
  }
}

function isOlderRevision(current: GuiConnectionItem, incoming: GuiConnectionItem): boolean {
  return current.revision !== undefined
    && incoming.revision !== undefined
    && incoming.revision < current.revision;
}

function mergeGuiConnection(
  current: GuiConnectionItem | undefined,
  incoming: GuiConnectionItem,
): GuiConnectionItem {
  if (!current) return incoming;
  const merged = { ...current } as Record<string, unknown>;
  for (const [key, value] of Object.entries(incoming)) {
    if (value !== undefined) merged[key] = value;
  }
  if (incoming.selectionChain.length === 0) merged['selectionChain'] = current.selectionChain;
  if (incoming.relayChain.length === 0) merged['relayChain'] = current.relayChain;
  return merged as unknown as GuiConnectionItem;
}

function awaitIgnore(promise: Promise<unknown>) {
  promise.catch(() => {});
}

export const coreEvents = new CoreEventsService();
