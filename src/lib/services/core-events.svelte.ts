import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { startGuiEvents, stopGuiEvents, appendLog, getCoreStats, getCoreRuntime } from '$lib/services/core';
import { overviewData } from '$lib/services/overview-data.svelte';
import { warning as showWarningToast } from '$lib/services/toast.svelte';
import type { CoreEventStatus, GuiEventPayload, TunStatusEvent, StackStatusEvent } from '$lib/types/core';
import type { GuiConnectionItem } from '$lib/types/gui-api';

const EVENT_NAME = 'gui:event';
const STATUS_NAME = 'gui:event-status';

// ── Exported types ──

export type ConnectionDelta =
  | { type: 'started'; connection: GuiConnectionItem }
  | { type: 'updated'; connection: GuiConnectionItem }
  | { type: 'closed'; flowId: string };

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
  private _activeGeneration: number | null = null;

  private _stopped = false;

  async start(events?: string[]) {
    this._stopped = false;

    // Listen before starting subscription so we don't miss status events
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

    try {
      const sub = await startGuiEvents(events);
      this._activeGeneration = sub.generation;
    } catch (e) {
      this.status = 'error';
      this.lastError = String(e);
    }
  }

  stop() {
    this._stopped = true;
    stopGuiEvents();
    this._activeGeneration = null;
    this.isSubscribed = false;
    this.status = 'idle';
    this._unlistenEvent?.();
    this._unlistenStatus?.();
    this._unlistenProcess?.();
    this._unlistenEvent = null;
    this._unlistenStatus = null;
    this._unlistenProcess = null;
    this._pendingDeltas = [];
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
    if (status.generation !== this._activeGeneration) return;

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

    if (eventType === 'policy.selected' || eventType === 'policy.probeCompleted') {
      awaitIgnore(overviewData.refreshPolicyNodes());
      // Bump statusTick so policy-group watchers (node page, overview)
      // re-fetch runtime data and reflect the new selection / latency.
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
      const flowId = this._extractFlowId(data);
      if (flowId) {
        this._pushDelta({ type: 'closed', flowId });
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

    showWarningToast(message, 6000);
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
    this._pendingDeltas.push(delta);
    this._deltaSeq++;
  }

  private _parseConnectionEvent(data: unknown): GuiConnectionItem | null {
    if (!data || typeof data !== 'object') return null;
    const o = data as Record<string, unknown>;
    const flowId = typeof o['flowId'] === 'string' ? o['flowId'] : null;
    if (!flowId) return null;

    return {
      flowId,
      network: typeof o['network'] === 'string' ? o['network'] : 'tcp',
      source: typeof o['source'] === 'string' ? o['source'] : undefined,
      destination: typeof o['destination'] === 'string' ? o['destination'] : '-',
      inboundTag: typeof o['inboundTag'] === 'string' ? o['inboundTag'] : undefined,
      outboundTag: typeof o['outboundTag'] === 'string' ? o['outboundTag'] : undefined,
      policyTag: typeof o['policyTag'] === 'string' ? o['policyTag'] : undefined,
      routeMode: typeof o['routeMode'] === 'string' ? o['routeMode'] : undefined,
      outcome: typeof o['outcome'] === 'string' ? o['outcome'] : undefined,
      bytesUp: typeof o['bytesUp'] === 'number' ? o['bytesUp'] : 0,
      bytesDown: typeof o['bytesDown'] === 'number' ? o['bytesDown'] : 0,
      throughputUpBps: typeof o['throughputUpBps'] === 'number' ? o['throughputUpBps'] : undefined,
      throughputDownBps: typeof o['throughputDownBps'] === 'number' ? o['throughputDownBps'] : undefined,
      startedAtUnixMs: typeof o['startedAtUnixMs'] === 'number' ? o['startedAtUnixMs'] : undefined,
      updatedAtUnixMs: typeof o['updatedAtUnixMs'] === 'number' ? o['updatedAtUnixMs'] : undefined,
      durationMs: typeof o['durationMs'] === 'number' ? o['durationMs'] : undefined,
    };
  }

  private _extractFlowId(data: unknown): string | null {
    if (!data || typeof data !== 'object') return null;
    const o = data as Record<string, unknown>;
    return typeof o['flowId'] === 'string' ? o['flowId'] : null;
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

    if (stats && typeof stats === 'object') {
      overviewData.applyStatsEvent(stats as Record<string, unknown>);
    }
    if (runtime && typeof runtime === 'object') {
      overviewData.applyRuntimeEvent(runtime as Record<string, unknown>);
    }
    if (policies) {
      overviewData.applyPolicyEvent(policies);
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
      if (statsResult.available && statsResult.response) {
        overviewData.applyStatsEvent(statsResult.response as Record<string, unknown>);
      }
      if (runtimeResult.available && runtimeResult.response) {
        overviewData.applyRuntimeEvent(runtimeResult.response as Record<string, unknown>);
      }
      await overviewData.refreshPolicyNodes();
    } catch {
      // Best-effort initial fetch
    }
  }
}

function awaitIgnore(promise: Promise<unknown>) {
  promise.catch(() => {});
}

export const coreEvents = new CoreEventsService();
