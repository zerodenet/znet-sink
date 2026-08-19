export interface RuntimeProcessMetrics {
  role: 'gui' | 'core';
  label: string;
  pid: number | null;
  tracked: boolean;
  cpuPercent: number | null;
  memoryBytes: number | null;
}

export interface RuntimePerformanceSnapshot {
  sampledAtUnixMs: number;
  totalCpuPercent: number | null;
  totalMemoryBytes: number | null;
  processCount: number;
  trackedProcessCount: number;
  partial: boolean;
  gui: RuntimeProcessMetrics;
  core: RuntimeProcessMetrics | null;
}
