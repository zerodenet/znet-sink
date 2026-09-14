import type { TraceRouteResult } from '$lib/types/diagnostics';
import type { ToolJobSnapshot } from '$lib/types/gui-api';
import { ToolJobsState, type ToolJobPorts } from '$lib/features/tool-jobs/state.svelte';

interface Ports extends ToolJobPorts {
  getAppErrorMessage(error: unknown, fallback?: string): string;
}

export class RouteTraceState {
  traceTarget = $state('');
  tracePort = $state(80);
  traceProtocol = $state('');
  traceInboundTag = $state('');
  traceResult = $state<TraceRouteResult | null>(null);
  traceError = $state<string | null>(null);
  readonly jobs: ToolJobsState;
  private readonly ports: Ports;
  private starting = $state(false);

  constructor(ports: Ports) {
    this.ports = ports;
    this.jobs = new ToolJobsState(['route_trace'], ports, (job) => this.applyJob(job));
    void this.jobs.init();
  }

  get traceLoading(): boolean {
    return this.starting || this.jobs.active('route_trace').length > 0;
  }

  dispose(): void {
    this.jobs.dispose();
  }

  async runTrace(): Promise<void> {
    const target = this.traceTarget.trim();
    if (!target || this.traceLoading) return;
    this.traceError = null;
    this.traceResult = null;
    this.starting = true;
    try {
      await this.jobs.start({
        kind: 'route_trace',
        params: {
          target,
          port: this.tracePort || 80,
          ...(this.traceProtocol.trim() ? { protocol: this.traceProtocol.trim() } : {}),
          ...(this.traceInboundTag.trim() ? { inboundTag: this.traceInboundTag.trim() } : {}),
        },
      });
    } catch (error) {
      this.traceError = this.ports.getAppErrorMessage(error, '路由追踪任务启动失败');
    } finally {
      this.starting = false;
    }
  }

  async cancel(jobId: number): Promise<void> {
    try {
      await this.jobs.cancel(jobId);
    } catch (error) {
      this.traceError = this.ports.getAppErrorMessage(error, '取消路由追踪失败');
    }
  }

  private applyJob(job: ToolJobSnapshot): void {
    if (this.jobs.latest('route_trace')?.id !== job.id) return;
    if (job.state === 'completed') {
      this.traceResult = job.result as TraceRouteResult;
      this.traceError = null;
      return;
    }
    const messages: Partial<Record<ToolJobSnapshot['state'], string>> = {
      failed: job.error?.message ?? '路由追踪失败',
      timed_out: '路由追踪任务超时',
      cancelled: '路由追踪任务已取消',
      invalidated_by_config_change: '配置已切换，旧路由追踪结果已失效',
      invalidated_by_core_restart: '内核已重启，旧路由追踪结果已失效',
    };
    this.traceError = messages[job.state] ?? null;
  }
}
