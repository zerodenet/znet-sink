import type { TraceRouteResult } from '$lib/types/diagnostics';
import type { ToolJobSnapshot } from '$lib/types/gui-api';
import { ToolJobsState, type ToolJobPorts } from '$lib/features/tool-jobs/state.svelte';
import { parseRouteEvaluation, routeCompatibilityError } from './result';

interface Ports extends ToolJobPorts {
  getAppErrorMessage(error: unknown, fallback?: string): string;
}

export class RouteTraceState {
  traceTarget = $state('');
  tracePort = $state(80);
  traceProtocol = $state('tcp');
  traceInboundTag = $state('');
  traceResult = $state<TraceRouteResult | null>(null);
  traceError = $state<string | null>(null);
  readonly jobs: ToolJobsState;
  private readonly ports: Ports;
  private starting = $state(false);

  constructor(ports: Ports) {
    this.ports = ports;
    this.jobs = new ToolJobsState(['route_trace'], ports, (job) => this.applyJob(job), () => {
      this.traceResult = null; this.traceError = null; this.starting = false; this.traceInboundTag = '';
    });
    void this.jobs.init();
  }

  get traceLoading(): boolean {
    return this.starting || this.jobs.active('route_trace').length > 0;
  }

  dispose(): void {
    this.jobs.dispose();
  }

  async runTrace(): Promise<void> {
    if (!this.jobs.ready || this.jobs.disposed) return;
    const target = this.traceTarget.trim();
    if (!target || this.traceLoading) return;
    if (!Number.isInteger(this.tracePort) || this.tracePort < 1 || this.tracePort > 65535
      || !['tcp', 'udp'].includes(this.traceProtocol)) {
      this.traceError = '端口必须为1–65535整数，协议仅支持 TCP 或 UDP';
      return;
    }
    this.traceError = null;
    this.traceResult = null;
    this.starting = true;
    const epoch = this.jobs.epoch;
    try {
      await this.jobs.start({
        kind: 'route_trace',
        params: {
          target,
          port: this.tracePort,
          ...(this.traceProtocol.trim() ? { protocol: this.traceProtocol.trim() } : {}),
          ...(this.traceInboundTag.trim() ? { inboundTag: this.traceInboundTag.trim() } : {}),
        },
      });
    } catch (error) {
      if (!this.jobs.disposed && epoch === this.jobs.epoch) this.traceError = this.ports.getAppErrorMessage(error, '路由评估任务启动失败');
    } finally {
      if (epoch === this.jobs.epoch) this.starting = false;
    }
  }

  async cancel(jobId: number): Promise<void> {
    const epoch = this.jobs.epoch;
    try {
      await this.jobs.cancel(jobId);
    } catch (error) {
      if (!this.jobs.disposed && epoch === this.jobs.epoch) this.traceError = this.ports.getAppErrorMessage(error, '取消路由评估失败');
    }
  }

  private applyJob(job: ToolJobSnapshot): void {
    if (this.jobs.latest('route_trace')?.id !== job.id) return;
    if (job.state === 'completed') {
      try { this.traceResult = parseRouteEvaluation(job.result); this.traceError = null; }
      catch { this.traceResult = null; this.traceError = routeCompatibilityError; }
      return;
    }
    const messages: Partial<Record<ToolJobSnapshot['state'], string>> = {
      failed: job.error?.message ?? '路由评估失败',
      timed_out: '路由评估任务超时',
      cancelled: '路由评估任务已取消',
      invalidated_by_config_change: '配置已切换，旧路由评估结果已失效',
      invalidated_by_core_restart: '内核已重启，旧路由评估结果已失效',
    };
    this.traceError = messages[job.state] ?? null;
  }
}
