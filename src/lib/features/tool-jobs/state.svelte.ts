import type { StartToolJobRequest, ToolJobKind, ToolJobSnapshot } from '$lib/types/gui-api';

export interface ToolJobPorts {
  start(request: StartToolJobRequest): Promise<ToolJobSnapshot>;
  list(kind?: ToolJobKind): Promise<ToolJobSnapshot[]>;
  cancel(jobId: number): Promise<ToolJobSnapshot>;
  subscribe(handler: (job: ToolJobSnapshot) => void): Promise<() => void>;
}

export class ToolJobsState {
  jobs = $state<Map<number, ToolJobSnapshot>>(new Map());
  error = $state<string | null>(null);
  disposed = false;
  private readonly kinds: ReadonlySet<ToolJobKind>;
  private readonly ports: ToolJobPorts;
  private unlisten: (() => void) | null = null;
  private generation = 0;
  private onUpdate?: (job: ToolJobSnapshot) => void;

  constructor(kinds: ToolJobKind[], ports: ToolJobPorts, onUpdate?: (job: ToolJobSnapshot) => void) {
    this.kinds = new Set(kinds);
    this.ports = ports;
    this.onUpdate = onUpdate;
  }

  async init(): Promise<void> {
    const generation = ++this.generation;
    try {
      const unlisten = await this.ports.subscribe((job) => this.apply(job));
      if (this.disposed || generation !== this.generation) {
        unlisten();
        return;
      }
      this.unlisten = unlisten;
      const jobs = await this.ports.list();
      if (this.disposed || generation !== this.generation) return;
      for (const job of jobs) this.apply(job);
      this.error = null;
    } catch (error) {
      if (!this.disposed && generation === this.generation) {
        this.error = error instanceof Error ? error.message : String(error);
      }
    }
  }

  async start(request: StartToolJobRequest): Promise<ToolJobSnapshot> {
    if (this.disposed) throw new Error('诊断任务页面已关闭');
    const job = await this.ports.start(request);
    this.apply(job);
    return job;
  }

  async cancel(jobId: number): Promise<ToolJobSnapshot> {
    const job = await this.ports.cancel(jobId);
    this.apply(job);
    return job;
  }

  apply(job: ToolJobSnapshot): void {
    if (this.disposed || !this.kinds.has(job.kind)) return;
    const current = this.jobs.get(job.id);
    if (current && current.updatedAtUnixMs > job.updatedAtUnixMs) return;
    const jobs = new Map(this.jobs);
    jobs.set(job.id, job);
    this.jobs = jobs;
    this.onUpdate?.(job);
  }

  latest(kind: ToolJobKind): ToolJobSnapshot | null {
    return [...this.jobs.values()]
      .filter((job) => job.kind === kind)
      .sort((left, right) => right.id - left.id)[0] ?? null;
  }

  active(kind?: ToolJobKind): ToolJobSnapshot[] {
    return [...this.jobs.values()].filter((job) => {
      if (kind && job.kind !== kind) return false;
      return job.state === 'queued' || job.state === 'running' || job.state === 'cancelling';
    });
  }

  dispose(): void {
    this.disposed = true;
    this.generation += 1;
    this.unlisten?.();
    this.unlisten = null;
  }
}
