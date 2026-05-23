import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { startGuiEvents, stopGuiEvents, appendLog, getCoreStats, getCoreRuntime } from '$lib/services/core';
import { overviewData } from '$lib/services/overview-data.svelte';
import type { CoreEventStatus, GuiEventPayload } from '$lib/types/core';

const EVENT_NAME = 'gui:event';
const STATUS_NAME = 'gui:event-status';

class CoreEventsService {
  isSubscribed = $state(false);
  status = $state<'idle' | 'subscribed' | 'offline' | 'error' | 'disconnected'>('idle');
  lastError = $state<string | null>(null);
  connectionTick = $state(0);

  private _unlistenEvent: UnlistenFn | null = null;
  private _unlistenStatus: UnlistenFn | null = null;
  private _activeGeneration: number | null = null;

  async start(events?: string[]) {
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

    try {
      const sub = await startGuiEvents(events);
      this._activeGeneration = sub.generation;
    } catch (e) {
      this.status = 'error';
      this.lastError = String(e);
    }
  }

  stop() {
    stopGuiEvents();
    this._activeGeneration = null;
    this.isSubscribed = false;
    this.status = 'idle';
    this._unlistenEvent?.();
    this._unlistenStatus?.();
    this._unlistenEvent = null;
    this._unlistenStatus = null;
  }

  private _handleStatus(status: CoreEventStatus) {
    if (status.generation !== this._activeGeneration) return;

    switch (status.status) {
      case 'subscribed':
        this.isSubscribed = true;
        this.status = 'subscribed';
        this.lastError = null;
        // Fetch initial state snapshot to fill gaps
        this._fetchInitialState();
        break;
      case 'offline':
        this.isSubscribed = false;
        this.status = 'offline';
        this.lastError = status.error?.message ?? 'core is not available';
        overviewData.isLive = false;
        break;
      case 'disconnected':
        this.isSubscribed = false;
        this.status = 'disconnected';
        overviewData.isLive = false;
        break;
      case 'stopped':
        this.isSubscribed = false;
        this.status = 'idle';
        break;
      case 'error':
        this.isSubscribed = false;
        this.status = 'error';
        this.lastError = status.error?.message ?? 'unknown error';
        overviewData.isLive = false;
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
      return;
    }

    if (eventType === 'core.configChanged') {
      awaitIgnore(this._fetchInitialState());
      return;
    }

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

  private _hasAnyKey(obj: Record<string, unknown>, keys: string[]): boolean {
    return keys.some((k) => k in obj);
  }

  private _eventData(payload: unknown): unknown {
    if (!payload || typeof payload !== 'object') return payload;
    const obj = payload as Record<string, unknown>;
    return 'data' in obj ? obj['data'] : payload;
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
