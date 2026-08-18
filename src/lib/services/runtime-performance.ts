import { invoke } from '@tauri-apps/api/core';

import type { CoreProcessStatus } from '$lib/types/core';
import type { RuntimePerformanceSnapshot } from '$lib/types/runtime-performance';

type CoreProcessStatusWithPerformance = CoreProcessStatus & {
  runtimePerformance?: RuntimePerformanceSnapshot;
};

export async function getRuntimePerformanceSnapshot(): Promise<RuntimePerformanceSnapshot> {
  const response = await invoke<CoreProcessStatusWithPerformance>('core_process_status', {
    includePerformance: true,
  });
  if (!response.runtimePerformance) {
    throw new Error('实时资源采样未返回数据');
  }
  return response.runtimePerformance;
}
