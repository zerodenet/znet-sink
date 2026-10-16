import type { ClientCoreSnapshot, ClientScope, StartToolJobRequest, ToolJobKind, ToolJobSnapshot } from '$lib/types/gui-api';

export interface ToolJobPorts {
  start(request: StartToolJobRequest): Promise<ToolJobSnapshot>;
  list(kind?: ToolJobKind): Promise<ToolJobSnapshot[]>;
  cancel(jobId: number, expectedScope?: ClientScope, expectedClientRevision?: number): Promise<ToolJobSnapshot>;
  subscribe(handler: (job: ToolJobSnapshot) => void): Promise<() => void>;
  scope(): Promise<ClientCoreSnapshot>;
  subscribeScope(handler: (snapshot: ClientCoreSnapshot | null) => void): Promise<() => void>;
}
const active = (job: ToolJobSnapshot) => ['queued', 'running', 'cancelling'].includes(job.state);
const stage = (job: ToolJobSnapshot) => job.state === 'queued' ? 0 : job.state === 'running' ? 1 : job.state === 'cancelling' ? 2 : 3;
const safeId = (id: number): boolean => Number.isSafeInteger(id) && id > 0;
const safeScope = (scope: ClientScope): boolean => {
  if (!Number.isSafeInteger(scope.coreInstanceId) || scope.coreInstanceId < 0) return false;
  const revision = scope.configRevision;
  if (typeof revision === 'number') return Number.isSafeInteger(revision) && revision >= 0;
  return /^(0|[1-9][0-9]*)$/.test(revision) && revision.length <= 20 && BigInt(revision) <= 18446744073709551615n;
};
const safeJob = (job: ToolJobSnapshot): boolean => safeId(job.id) && safeScope(job.scope);
export const sameToolScope = (left: ClientScope | null, right: ClientScope | null): boolean =>
  left !== null && right !== null && safeScope(left) && safeScope(right) && left.profileId === right.profileId
  && String(left.configRevision) === String(right.configRevision) && left.coreInstanceId === right.coreInstanceId;

/** Read projection only: the existing host runtime owns the queue and execution. */
export class ToolJobsState {
  jobs = $state<Map<number, ToolJobSnapshot>>(new Map());
  scope = $state<ClientScope | null>(null);
  ready = $state(false);
  epoch = $state(0);
  error = $state<string | null>(null);
  unsupportedIdentity = $state(false);
  disposed = false;
  private readonly kinds: ReadonlySet<ToolJobKind>;
  private readonly ports: ToolJobPorts;
  private unlisten: (() => void)[] = [];
  private generation = 0;
  private revision = -1;
  private scopeGeneration = -1;
  private observing = false;
  private initialized: Promise<void> | null = null;
  private projectable = new Set<number>();
  private revoked = new Set<number>();
  private admitted = new Map<number, number>();
  private published = new Map<number, string>();
  private historyReady = false;
  private onUpdate?: (job: ToolJobSnapshot) => void;
  private onScopeChange?: () => void;

  constructor(kinds: ToolJobKind[], ports: ToolJobPorts, onUpdate?: (job: ToolJobSnapshot) => void, onScopeChange?: () => void) {
    this.kinds = new Set(kinds);
    this.ports = ports;
    this.onUpdate = onUpdate;
    this.onScopeChange = onScopeChange;
  }

  init(): Promise<void> {
    if (!this.initialized) this.initialized = this.initialize();
    return this.initialized;
  }

  private async initialize(): Promise<void> {
    const generation = ++this.generation;
    const attach = (stop: () => void) => {
      if (this.disposed || generation !== this.generation) { stop(); return false; }
      this.unlisten.push(stop); return true;
    };
    try {
      if (!attach(await this.ports.subscribe(job => { void this.receive(job); }))) return;
      if (!attach(await this.ports.subscribeScope(snapshot => this.bind(snapshot)))) return;
      this.observing = true;
      try { await this.refreshScope(); } catch (error) { if (!this.ready) throw error; }
      this.historyReady = true;
      const epoch = this.epoch;
      try { await this.restoreActive(); } catch (error) { if (epoch === this.epoch) throw error; }
      if (this.disposed || generation !== this.generation) return;
    } catch (error) {
      if (!this.disposed && generation === this.generation) {
        this.bind(null);
        this.observing = false;
        this.historyReady = false;
        this.error = error instanceof Error ? error.message : String(error);
        this.unlisten.splice(0).forEach(stop => stop());
        this.initialized = null;
      }
    }
  }

  bind(snapshot: ClientCoreSnapshot | null): void {
    if (this.disposed) return;
    if (snapshot && (!Number.isSafeInteger(snapshot.revision) || snapshot.revision < 0
      || !Number.isSafeInteger(snapshot.scopeGeneration) || snapshot.scopeGeneration! < 1
      || !safeScope(snapshot.scope))) snapshot = null;
    if (snapshot && snapshot.revision < this.revision) return;
    if (snapshot) this.revision = snapshot.revision;
    const next = snapshot?.scope ?? null;
    const ready = this.observing && snapshot?.sourceStatus === 'ready';
    const generationChanged = !!snapshot && this.scopeGeneration !== snapshot.scopeGeneration;
    if (!sameToolScope(this.scope, next) || this.ready !== ready || generationChanged) {
      // A confirmed different scope revokes known IDs permanently for this
      // page, including A -> B -> A. An unreadable scope is not proof of a
      // transition; resumption must re-read actual active jobs instead.
      if (snapshot) for (const job of this.jobs.values()) if (!sameToolScope(job.scope, next)) this.revoked.add(job.id);
      if (generationChanged) for (const [id, generation] of this.admitted) if (generation !== snapshot!.scopeGeneration) this.revoked.add(id);
      if (snapshot) this.scopeGeneration = snapshot.scopeGeneration!;
      this.epoch++;
      this.projectable.clear();
      this.published.clear();
      this.scope = next ? { ...next } : null;
      this.ready = ready;
      this.onScopeChange?.();
      const epoch = this.epoch;
      if (ready && this.historyReady) void this.restoreActive().catch(() => { if (epoch === this.epoch) this.bind(null); });
    }
    this.error = snapshot ? (ready ? null : '当前运行来源未就绪；仅显示历史任务，不交付旧结果') : '无法核对当前任务范围；历史记录保留，运行结果不可用';
  }

  async refreshScope(): Promise<void> {
    const epoch = this.epoch;
    try { this.bind(await this.ports.scope()); }
    catch (error) { if (epoch === this.epoch) this.bind(null); throw error; }
  }

  private async receive(job: ToolJobSnapshot): Promise<void> {
    const epoch = this.epoch;
    try { await this.refreshScope(); }
    catch { this.apply(job, false); return; }
    if (epoch !== this.epoch) { this.apply(job, false); return; }
    if (active(job) && !this.projectable.has(job.id) && !this.revoked.has(job.id)) {
      try { await this.restoreActive(); } catch { this.apply(job, false); return; }
    }
    this.apply(job, epoch === this.epoch);
  }

  private async restoreActive(): Promise<void> {
    const epoch = this.epoch, scope = this.scope ? { ...this.scope } : null;
    const jobs = await this.ports.list();
    if (this.disposed) return;
    for (const job of jobs) {
      if (!safeJob(job)) { this.unsupportedIdentity = true; continue; }
      // Only this epoch's authoritative active read may grant recovery.
      // Do this before merging: a newer completion may already be retained.
      if (epoch === this.epoch && this.ready && sameToolScope(scope, this.scope)
        && sameToolScope(job.scope, scope) && active(job) && !this.revoked.has(job.id)) {
        this.projectable.add(job.id);
        this.admitted.set(job.id, this.scopeGeneration);
      }
      this.apply(job, false);
      if (epoch === this.epoch) this.project(job.id);
    }
  }

  async start(request: StartToolJobRequest): Promise<ToolJobSnapshot> {
    const invokedEpoch = this.ready ? this.epoch : null;
    const expected = request.expectedScope ?? (this.scope ? { ...this.scope } : null);
    await this.init();
    if (this.disposed) throw new Error('诊断任务页面已关闭');
    await this.refreshScope();
    if (!this.ready || !this.scope || (invokedEpoch !== null && invokedEpoch !== this.epoch) || (expected && !sameToolScope(expected, this.scope))) throw new Error('任务范围已变化或未就绪，请重新核对后发起');
    const scope = { ...this.scope }, epoch = this.epoch;
    const job = await this.ports.start({ ...request, expectedScope: scope, expectedClientRevision: this.revision });
    if (!safeJob(job)) {
      this.unsupportedIdentity = true;
      throw new Error('任务已提交，但宿主返回的身份无法无损表示；结果未确认，不会重放或停止舍入后的目标');
    }
    try { await this.refreshScope(); } catch { this.apply(job, false); throw new Error('任务已提交，但当前范围未确认；不会自动重放'); }
    if (this.disposed || epoch !== this.epoch || !sameToolScope(job.scope, scope)) {
      this.revoked.add(job.id);
      this.apply(job, false);
      throw new Error('任务已提交，但所属范围已变化；结果仅保留为历史，不会重放');
    }
    this.projectable.add(job.id);
    this.admitted.set(job.id, this.scopeGeneration);
    this.apply(job, true);
    this.project(job.id);
    return job;
  }

  async cancel(jobId: number): Promise<ToolJobSnapshot> {
    if (!safeId(jobId)) throw new Error('任务身份无法无损表示，不能停止舍入后的目标');
    const epoch = this.epoch;
    const expected = this.jobs.get(jobId)?.scope;
    if (!expected || !active(this.jobs.get(jobId)!)) throw new Error('仅能停止当前范围的活动任务');
    await this.refreshScope();
    if (this.disposed || !this.ready || epoch !== this.epoch || this.revoked.has(jobId) || !sameToolScope(expected, this.scope)) throw new Error('历史任务范围已失效，不能继续控制');
    const job = await this.ports.cancel(jobId, expected, this.revision);
    await this.refreshScope();
    this.apply(job, epoch === this.epoch);
    return job;
  }

  apply(job: ToolJobSnapshot, live = false): void {
    if (this.disposed || !this.kinds.has(job.kind)) return;
    if (!safeJob(job)) { this.unsupportedIdentity = true; return; }
    const current = this.jobs.get(job.id);
    if (current && (current.updatedAtUnixMs > job.updatedAtUnixMs || (current.updatedAtUnixMs === job.updatedAtUnixMs && (current.state === job.state || stage(current) > stage(job))))) return;
    const jobs = new Map(this.jobs);
    jobs.set(job.id, job);
    const retained = [...jobs.values()].sort((a, b) => Number(active(b)) - Number(active(a)) || b.id - a.id).slice(0, 128);
    this.jobs = new Map(retained.map(job => [job.id, job]));
    for (const id of this.projectable) if (!this.jobs.has(id)) { this.projectable.delete(id); this.published.delete(id); }
    for (const id of this.revoked) if (!this.jobs.has(id)) this.revoked.delete(id);
    for (const id of this.admitted.keys()) if (!this.jobs.has(id)) this.admitted.delete(id);
    if (live) this.project(job.id);
  }

  private project(id: number): void {
    const job = this.jobs.get(id);
    if (!job || !this.ready || this.revoked.has(id) || !sameToolScope(job.scope, this.scope) || !this.projectable.has(id)) return;
    const version = `${job.updatedAtUnixMs}:${job.state}`;
    if (this.published.get(id) === version) return;
    this.published.set(id, version);
    this.onUpdate?.(job);
  }

  latest(kind: ToolJobKind): ToolJobSnapshot | null {
    return [...this.jobs.values()].filter(job => job.kind === kind && this.current(job)).sort((a, b) => b.id - a.id)[0] ?? null;
  }

  private current(job: ToolJobSnapshot): boolean { return this.ready && !this.revoked.has(job.id) && this.projectable.has(job.id) && sameToolScope(job.scope, this.scope); }

  active(kind?: ToolJobKind): ToolJobSnapshot[] {
    return [...this.jobs.values()].filter(job => this.current(job) && (!kind || job.kind === kind) && active(job));
  }

  history(): ToolJobSnapshot[] { return [...this.jobs.values()].filter(job => !active(job) || !this.current(job)).sort((a, b) => b.id - a.id); }

  dispose(): void {
    this.disposed = true;
    this.generation++;
    this.epoch++;
    this.ready = false;
    this.onScopeChange?.();
    this.unlisten.splice(0).forEach(stop => stop());
  }
}
