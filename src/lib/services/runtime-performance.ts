import { invoke } from '@tauri-apps/api/core';

import type { CoreProcessStatus } from '$lib/types/core';
import type { RuntimePerformanceSnapshot } from '$lib/types/runtime-performance';

type CoreProcessStatusWithPerformance = CoreProcessStatus & {
  runtimePerformance?: RuntimePerformanceSnapshot;
};

export async function getRuntimePerformanceSnapshot(): Promise<RuntimePerformanceSnapshot> {
  const response = await invoke<CoreProcessStatusWithPerformance>('core_process_status', {
    includePerformance: true,
    // Thread enumeration is intentionally disabled for the desktop monitor.
    // CPU and RSS are cheap per-process reads; Windows thread counts require a
    // system-wide ToolHelp snapshot and do not provide enough user value to
    // justify that recurring cost.
    includePerformanceThreads: false,
  });
  if (!response.runtimePerformance) {
    throw new Error('资源占用读取未返回数据');
  }
  return response.runtimePerformance;
}
