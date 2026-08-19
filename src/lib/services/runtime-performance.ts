import { invoke } from '@tauri-apps/api/core';

import type { CoreProcessStatus } from '$lib/types/core';
import type { RuntimePerformanceSnapshot } from '$lib/types/runtime-performance';

type CoreProcessStatusWithPerformance = CoreProcessStatus & {
  runtimePerformance?: RuntimePerformanceSnapshot;
};

export async function getRuntimePerformanceSnapshot(
  includeThreads = false,
): Promise<RuntimePerformanceSnapshot> {
  const response = await invoke<CoreProcessStatusWithPerformance>('core_process_status', {
    includePerformance: true,
    includePerformanceThreads: includeThreads,
  });
  if (!response.runtimePerformance) {
    throw new Error('资源占用读取未返回数据');
  }
  return response.runtimePerformance;
}
