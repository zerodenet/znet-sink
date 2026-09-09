import { NodeScreenState } from '$lib/features/nodes/screen.svelte';
import type { NodeScreenSnapshot, ProbeJobSnapshot, StartProbeRequest } from '$lib/types/gui-api';
import { applyProbeJobSnapshot } from '$lib/features/probes/reconcile';
/** Optional manual-probe projection; execution is injected at the product boundary. */
export class ProbeJobsState extends NodeScreenState {
  directProbeJobs = $state<Map<number, ProbeJobSnapshot>>(new Map());
  terminalProbeJobIds = $state<Set<number>>(new Set());
  readonly reportedProbeJobs = new Set<number>();
  constructor(query: (reason: string) => Promise<NodeScreenSnapshot>, private execute?: (request: StartProbeRequest) => Promise<ProbeJobSnapshot>, private cancelJob?: (id: number) => Promise<ProbeJobSnapshot>) { super(query); }
  async start(request: StartProbeRequest) {
    if (this.disposed || !this.execute) throw new Error('测速模块未就绪');
    const job = await this.execute(request);
    this.apply(job);
    return job;
  }
  async cancel(id: number) {
    if (this.disposed || !this.cancelJob) throw new Error('测速模块未就绪');
    const job = await this.cancelJob(id);
    this.apply(job);
    return job;
  }
  apply(job: ProbeJobSnapshot) {
    if (this.disposed) return;
    const next = applyProbeJobSnapshot({directJobs: this.directProbeJobs, terminalJobIds: this.terminalProbeJobIds}, job);
    this.directProbeJobs = next.directJobs; this.terminalProbeJobIds = next.terminalJobIds;
  }
  // Page disposal releases subscriptions only; backend tasks retain ownership.
}
