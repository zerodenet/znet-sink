import type { TrafficDiscovery, TrafficGateway, TrafficSnapshot, TrafficPage, TrafficEvent, StreamStatus, ResetInput, ResetSnapshot } from '$lib/features/traffic/types';
import { scopeKey } from '$lib/features/traffic/types';
import { supported, samplingSupported, resetReason, errorCode } from '$lib/features/traffic/policy';
import { inventory } from '$lib/features/traffic/inventory';
import { observe, rebaseline, type Observation } from '$lib/features/traffic/history';

export const MAX_HISTORY_SCOPES = 64;
export interface TrafficView {
  discovery: TrafficDiscovery | null;
  rows: Record<string, Observation>;
  order: string[];
  loading: boolean;
  stale: boolean;
  streaming: boolean;
  resetting: boolean;
  error: unknown;
  result: string | null;
}
/** One inventory owner, one bounded history, no per-card network polling. */
export class TrafficSession {
  view: TrafficView = { discovery: null, rows: {}, order: [], loading: false, stale: true, streaming: false, resetting: false, error: null, result: null };
  private alive = true;
  private revision = 0;
  private stop?: () => void;
  private timer?: ReturnType<typeof setInterval>;
  private work?: Promise<void>;
  private watched = new Set<string>();
  private retiredInstances: string[] = [];
  private retiredEpochs = new Map<string, string[]>();
  private instance?: string;
  private config?: string;
  private registry?: string;
  private lastQuery = 0;
  private lastAttempt = 0;
  private pendingRecovery = false;
  private events = new Map<string, TrafficSnapshot>();
  private permissionDenied = false;
  private subscribed = false;
  private gateway: TrafficGateway;
  private publish: (view: TrafficView) => void;
  private now: () => number;
  constructor(gateway: TrafficGateway, publish: (view: TrafficView) => void, now = () => Date.now()) {
    this.gateway = gateway; this.publish = publish; this.now = now;
  }
  private emit(patch: Partial<TrafficView>) {
    if (!this.alive) return;
    this.view = { ...this.view, ...patch };
    this.publish(this.view);
  }
  async start() {
    await this.refresh(true);
    if (!this.alive) return;
    try { this.stop = await this.gateway.subscribe(e => this.event(e), s => this.status(s)); }
    catch { this.status('offline'); }
    if (!this.alive) { this.stop?.(); return; }
    this.timer = setInterval(() => this.tick(), 1000);
  }
  watch(keys: string[]) {
    this.watched = new Set(keys.slice(0, MAX_HISTORY_SCOPES));
    const rows = { ...this.view.rows };
    let changed = false;
    for (const [key, row] of Object.entries(rows)) if (!this.watched.has(key) && row.points.length) {
      rows[key] = { ...row, points: [] }; changed = true;
    }
    if (changed) this.emit({ rows });
  }
  private tick() {
    if (!this.alive || this.view.resetting || this.work) return;
    if (!supported(this.view.discovery)) {
      if (this.now() - this.lastAttempt >= (this.view.discovery ? 30_000 : 3000)) void this.refresh(true);
      return;
    }
    const cap = this.view.discovery!.capabilities.trafficStatistics!;
    const round = Math.ceil(Math.max(1, this.view.order.length) / Math.max(1, cap.sample_page_size)) * Math.max(1000, cap.sample_interval_ms);
    const staleAfter = Math.max(10_000, round * 3);
    const missing = this.view.streaming && this.view.order.some(k => this.now() - this.view.rows[k].seenAt > staleAfter);
    if (missing) this.invalidate();
    if ((missing || this.now() - this.lastQuery >= (this.view.streaming ? 30_000 : 3000)) && this.now() - this.lastAttempt >= 3000) void this.refresh();
  }
  private invalidate() {
    const rows = Object.fromEntries(Object.entries(this.view.rows).map(([key, row]) => [key, rebaseline(row)]));
    this.revision++;
    this.emit({ rows, stale: true });
  }
  private status(state: StreamStatus) {
    if (state !== 'gap') this.subscribed = state === 'subscribed';
    const streaming = this.subscribed && samplingSupported(this.view.discovery);
    if (streaming !== this.view.streaming || state === 'gap' || state === 'reconnecting' || state === 'subscribed') {
      this.emit({ streaming });
      this.invalidate();
      this.pendingRecovery = true;
      if (!this.view.resetting) void this.refresh(state === 'subscribed');
    }
  }
  refresh(rediscover = false): Promise<void> {
    if (!this.alive) return Promise.resolve();
    if (this.view.resetting) { this.pendingRecovery = true; return Promise.resolve(); }
    if (this.work) { if (rediscover) this.pendingRecovery = true; return this.work; }
    const revision = this.revision;
    this.lastAttempt = this.now();
    this.emit({ loading: true });
    this.work = Promise.resolve().then(async () => {
      try {
        if (rediscover || !this.view.discovery) {
          const discovery = await this.gateway.discover();
          if (!this.alive || revision !== this.revision) return;
          if (this.permissionDenied) discovery.admin = false;
          this.emit({ discovery, streaming: this.subscribed && samplingSupported(discovery) });
        }
        if (!supported(this.view.discovery)) {
          this.emit({ rows: {}, order: [], stale: false, error: null });
          return;
        }
        const maximum = this.view.discovery!.capabilities.trafficStatistics!.maximum_page_size;
        if (!Number.isSafeInteger(maximum) || maximum < 1) throw new Error('内核分页能力无效');
        const page = await inventory(this.gateway, Math.min(64, maximum), () => this.alive && revision === this.revision);
        if (!this.alive || revision !== this.revision) return;
        this.acceptInventory(page);
        this.lastQuery = this.now();
        this.emit({ stale: false, error: null });
      } catch (error) {
        if (this.alive && revision === this.revision) {
          this.invalidate(); this.emit({ error });
          if (errorCode(error) === 'unsupported') {
            this.emit({ discovery: null, streaming: false });
            this.pendingRecovery = !rediscover;
          }
        }
      } finally {
        this.work = undefined;
        this.emit({ loading: false });
        if (this.alive && this.pendingRecovery && !this.view.resetting) {
          this.pendingRecovery = false;
          void this.refresh(true);
        } else if (this.alive && !this.view.stale && !this.view.resetting) {
          const events = [...this.events.values()]; this.events.clear();
          for (const snapshot of events) this.sample(snapshot);
        }
      }
    });
    return this.work;
  }
  private retire(key: string, epoch: string) {
    const epochs = this.retiredEpochs.get(key) ?? [];
    if (!epochs.includes(epoch)) this.retiredEpochs.set(key, [...epochs, epoch].slice(-8));
  }
  private acceptInventory(page: TrafficPage) {
    if (this.retiredInstances.includes(page.core_instance_id)) throw new Error('忽略已退出内核实例的快照');
    if (this.instance && this.instance !== page.core_instance_id) {
      this.retiredInstances = [...this.retiredInstances, this.instance].slice(-4);
      this.retiredEpochs.clear(); this.events.clear();
      this.emit({ rows: {}, order: [] });
    }
    for (const snapshot of page.scopes) if (this.retiredEpochs.get(scopeKey(snapshot.scope))?.includes(snapshot.stats_epoch)) throw new Error('忽略已结束统计周期的快照');
    this.instance = page.core_instance_id; this.config = page.config_revision; this.registry = page.registry_revision;
    const rows: Record<string, Observation> = {};
    for (const snapshot of page.scopes) {
      const key = scopeKey(snapshot.scope), previous = this.view.rows[key];
      const same = previous?.snapshot.stats_epoch === snapshot.stats_epoch && previous.snapshot.generation === snapshot.generation;
      if (previous && previous.snapshot.stats_epoch !== snapshot.stats_epoch) this.retire(key, previous.snapshot.stats_epoch);
      rows[key] = same && previous.baselineValid && BigInt(snapshot.sampled_at_monotonic_ns) <= BigInt(previous.snapshot.sampled_at_monotonic_ns)
        ? previous : observe(previous, snapshot, this.watched.has(key), this.now());
    }
    // Only a complete, consistent inventory can remove identities.
    for (const key of this.retiredEpochs.keys()) if (!rows[key]) this.retiredEpochs.delete(key);
    this.emit({ rows, order: page.scopes.map(row => scopeKey(row.scope)) });
  }
  private event(event: TrafficEvent) {
    if (!this.alive || this.retiredInstances.includes(event.instance) || (this.view.discovery && !supported(this.view.discovery))) return;
    const snapshots = event.type === 'sample' ? (event.payload as TrafficPage).scopes : (event.payload as ResetSnapshot).snapshots;
    if (event.instance !== this.instance || (event.type === 'sample' && ((event.payload as TrafficPage).config_revision !== this.config || (event.payload as TrafficPage).registry_revision !== this.registry))) {
      this.recover(); return;
    }
    for (const snapshot of snapshots) {
      if (this.work || this.view.resetting || this.view.stale) {
        if (this.events.size < 16384) this.events.set(scopeKey(snapshot.scope), snapshot);
        else this.pendingRecovery = true;
      } else this.sample(snapshot);
    }
  }
  private sample(snapshot: TrafficSnapshot) {
    const key = scopeKey(snapshot.scope), previous = this.view.rows[key];
    if (this.retiredEpochs.get(key)?.includes(snapshot.stats_epoch)) return;
    if (!previous || snapshot.stats_epoch !== previous.snapshot.stats_epoch || snapshot.generation !== previous.snapshot.generation || snapshot.config_revision !== this.config) {
      this.recover(); return;
    }
    if (BigInt(snapshot.sampled_at_monotonic_ns) <= BigInt(previous.snapshot.sampled_at_monotonic_ns)) return;
    const next = observe(previous, snapshot, this.watched.has(key), this.now());
    this.emit({ rows: { ...this.view.rows, [key]: next } });
  }
  private recover() {
    if (!this.view.stale) this.invalidate();
    this.pendingRecovery = true;
    if (!this.work && !this.view.resetting) { this.pendingRecovery = false; void this.refresh(true); }
  }
  plan(keys: string[]): ResetInput {
    if (this.view.stale || this.view.loading || this.view.resetting || !this.instance) throw new Error('请先完成统计同步');
    const unique = [...new Set(keys)];
    const cap = this.view.discovery?.capabilities.trafficStatistics;
    if (!unique.length || unique.length > Math.min(256, cap?.maximum_reset_targets ?? 0)) throw new Error('请选择适用的统计范围');
    const targets = unique.map(key => {
      const snapshot = this.view.rows[key]?.snapshot;
      if (!snapshot) throw new Error('统计范围已不存在');
      const reason = resetReason(this.view.discovery, snapshot);
      if (reason) throw new Error(reason);
      return { scope: snapshot.scope, expected_stats_epoch: snapshot.stats_epoch, ...(snapshot.generation !== null ? { expected_generation: snapshot.generation } : {}) };
    });
    return { expected_core_instance_id: this.instance, operation_id: crypto.randomUUID(), targets };
  }
  async reset(plan: ResetInput) {
    if (!this.alive || this.view.resetting || this.work) return;
    this.emit({ resetting: true, result: null, error: null });
    this.revision++;
    try {
      const receipt = await this.gateway.reset(plan);
      if (!this.alive) return;
      this.validateReceipt(receipt, plan);
      const rows = { ...this.view.rows };
      for (const snapshot of receipt.snapshots) {
        const key = scopeKey(snapshot.scope);
        if (rows[key]) this.retire(key, rows[key].snapshot.stats_epoch);
        rows[key] = observe(undefined, snapshot, this.watched.has(key), this.now());
      }
      this.emit({ rows, result: `已清空 ${receipt.snapshots.length} 个统计范围` });
    } catch (error) {
      if (!this.alive) return;
      if (errorCode(error) === 'permission_denied') {
        this.permissionDenied = true;
        if (this.view.discovery) this.emit({ discovery: { ...this.view.discovery, admin: false } });
      }
      const rows = { ...this.view.rows };
      for (const target of plan.targets) {
        const key = scopeKey(target.scope); if (rows[key]) rows[key] = rebaseline(rows[key]);
      }
      const refusals: Record<string, string> = {
        permission_denied: '权限不足，无法清空统计',
        conflict: '统计实例、周期或版本已变化，请重新选择',
        not_found: '所选统计范围已不存在',
        invalid_argument: '清空请求不适用于当前统计范围',
        unsupported: '当前内核不支持此统计清空操作',
      };
      const reason = refusals[errorCode(error) ?? ''];
      this.emit({ rows, stale: true, error, result: reason
        ? `${reason}。正在查询当前统计，不会自动重试。`
        : '清空未获确认，正在查询当前统计。不会自动重试。' });
    } finally {
      this.events.clear();
      this.emit({ resetting: false });
      // Recover authoritative state even if acknowledgement was lost. Never
      // create a replacement reset plan or resend the submitted command.
      if (this.alive) { this.pendingRecovery = false; await this.refresh(); }
    }
  }
  private validateReceipt(receipt: ResetSnapshot, plan: ResetInput) {
    if (receipt.core_instance_id !== plan.expected_core_instance_id || receipt.operation_id !== plan.operation_id || receipt.snapshots.length !== plan.targets.length) throw new Error('清空确认不匹配');
    const found = new Set<string>();
    for (const snapshot of receipt.snapshots) {
      const key = scopeKey(snapshot.scope), target = plan.targets.find(t => scopeKey(t.scope) === key);
      if (!target || found.has(key) || snapshot.core_instance_id !== plan.expected_core_instance_id || snapshot.stats_epoch === target.expected_stats_epoch || (target.expected_generation !== undefined && snapshot.generation !== target.expected_generation)) throw new Error('清空确认的范围、周期或 generation 不匹配');
      found.add(key);
    }
  }
  dispose() {
    this.alive = false; this.revision++;
    this.stop?.(); if (this.timer) clearInterval(this.timer);
    this.events.clear(); this.retiredEpochs.clear(); this.watched.clear();
  }
}
