import { registerModule } from '$lib/features/diagnostics/registry';
import type { TraceRouteResult } from '$lib/types/diagnostics';
import type * as Core from './client';
import type { getAppErrorMessage } from '$lib/services/core';
interface Ports {
  guiTraceRoute: typeof Core.guiTraceRoute;
  getAppErrorMessage: typeof getAppErrorMessage;
}
export class RouteTraceState {
  traceTarget = $state('');
  tracePort = $state(80);
  traceProtocol = $state('');
  traceInboundTag = $state('');
  traceLoading = $state(false);
  traceResult = $state<TraceRouteResult | null>(null);
  traceError = $state<string | null>(null);
  private releaseDiagnostic = registerModule('route-trace', () => ({id: 'route-trace', title: '路由追踪', state: this.traceLoading ? 'busy' : this.traceError ? 'error' : this.traceResult ? 'ready' : 'idle', summary: '按需读取路由诊断', error: this.traceError, facts: []}));
  private generation = 0;
  private ports: Ports;
  constructor(ports: Ports) { this.ports = ports; }
  dispose() {
    this.releaseDiagnostic(); this.generation++; this.traceLoading = false; }
  async runTrace() {
    const target = this.traceTarget.trim();
    if (!target || this.traceLoading) return;
    this.traceLoading = true;
    this.traceError = null;
    this.traceResult = null;
    const generation = this.generation;
    try {
      const proto = this.traceProtocol.trim() || undefined;
      const inboundTag = this.traceInboundTag.trim() || undefined;
      const result = await this.ports.guiTraceRoute(target, this.tracePort || undefined, proto, inboundTag);
      if (generation === this.generation) this.traceResult = result;
    } catch (e) {
      if (generation !== this.generation) return;
      this.traceError = this.ports.getAppErrorMessage(e, '路由追踪失败');
    } finally {
      if (generation === this.generation) {
        this.traceLoading = false;
      }
    }
  }

}
