import { registerModule } from '$lib/features/diagnostics/registry';
import { NodeScreenState } from '$lib/features/nodes/screen.svelte';
import type { NodeScreenSnapshot, ProbeJobSnapshot, StartProbeRequest } from '$lib/types/gui-api';
import { applyProbeJobSnapshot } from '$lib/features/probes/reconcile';
/** Optional manual-probe projection; execution is injected at the product boundary. */
export class ProbeJobsState extends NodeScreenState {
  directProbeJobs = $state<Map<number, ProbeJobSnapshot>>(new Map());
  terminalProbeJobIds = $state<Set<number>>(new Set());
  readonly reportedProbeJobs = new Set<number>();
  private releaseDiagnostic = registerModule('probe-jobs', () => ({id: 'probe-jobs', title: '节点测速任务', state: this.directProbeJobs.size ? 'busy' : this.error ? 'error' : this.nodeScreen ? 'ready' : 'idle', summary: `${this.directProbeJobs.size} 个前端活动任务投影`, error: this.error, facts: []}));
  constructor(query: (reason: string) => Promise<NodeScreenSnapshot>, private execute?: (request: StartProbeRequest) => Promise<ProbeJobSnapshot>) { super(query); }
  async start(request: StartProbeRequest) {
    if (this.disposed || !this.execute) throw new Error('测速模块未就绪');
    const job = await this.execute(request);
    this.apply(job);
    return job;
  }
  apply(job: ProbeJobSnapshot) {
    if (this.disposed) return;
    const next = applyProbeJobSnapshot({directJobs: this.directProbeJobs, terminalJobIds: this.terminalProbeJobIds}, job);
    this.directProbeJobs = next.directJobs; this.terminalProbeJobIds = next.terminalJobIds;
  }
  override dispose() { this.releaseDiagnostic(); super.dispose(); }
}
