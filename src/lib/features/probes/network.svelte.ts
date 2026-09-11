import type { NetworkProbeResult } from '$lib/services/core';
interface Ports { query(): Promise<NetworkProbeResult>; changed(): void; error(error: unknown): string; }
export class NetworkProbeState {
  result = $state<NetworkProbeResult | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  updatedAt = $state(0);
  private generation = 0;
  private pending = false;
  private timer: ReturnType<typeof setInterval> | null = null;
  private ports: Ports;
  constructor(ports: Ports) { this.ports = ports; }
  start(interval = 300000) {
    if (this.timer !== null) return;
    this.timer = setInterval(() => { void this.run(); }, interval);
  }
  stop() {
    this.generation++; this.pending = false; this.loading = false;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }
  async run() {
    if (this.loading) { this.pending = true; return; }
    const generation = this.generation;
    this.loading = true; this.pending = false; this.error = null;
    try {
      const result = await this.ports.query();
      if (generation === this.generation) { this.result = result; this.updatedAt = Date.now(); }
    } catch (error) {
      if (generation === this.generation) { this.result = null; this.error = this.ports.error(error); }
    } finally {
      if (generation === this.generation) {
        this.loading = false;
        this.ports.changed();
        if (this.pending) void this.run();
      }
    }
  }
}
