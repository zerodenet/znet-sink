import { getProbeRuntimeSnapshot, type ProbeRuntimeSnapshot } from './client';
import type { ModuleSource, ModuleStatus } from '$lib/features/diagnostics/model';
export function probeRuntimeStatus(runtime: ProbeRuntimeSnapshot): ModuleStatus {
  return {
    id: 'probe-jobs', title: '节点测速任务',
    state: runtime.jobs ? 'busy' : 'idle',
    summary: runtime.jobs ? `${runtime.jobs} 个任务持有执行资源` : '当前无测速任务',
    facts: [
      {label: '排队目标', value: String(runtime.queued)},
      {label: '执行请求', value: `${runtime.inFlight} / ${runtime.maxConcurrency}`},
      {label: '停止后仍在等待', value: String(runtime.draining)},
      {label: '目标容量', value: `${runtime.queued + runtime.inFlight} / ${runtime.maxPendingTargets}`},
      {label: '关闭节点页', value: '任务继续；重新打开可恢复进度'},
      {label: '停止测速', value: '清除排队目标；内核不支持取消已发送请求，等待返回或超时'},
    ],
  };
}
export const probeDiagnostics: ModuleSource[] = [{
  id: 'probe-jobs', title: '节点测速任务',
  read: async () => probeRuntimeStatus(await getProbeRuntimeSnapshot()),
}];
