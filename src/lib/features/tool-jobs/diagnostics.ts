import type { ModuleSource, ModuleStatus } from '$lib/features/diagnostics/model';
import type { ToolJobKind, ToolRuntimeSnapshot } from '$lib/types/gui-api';
import { getToolRuntimeSnapshot } from './client';

function status(
  runtime: ToolRuntimeSnapshot,
  id: string,
  title: string,
  kinds: ToolJobKind[],
  budget: string,
): ModuleStatus {
  const entries = runtime.kinds.filter((entry) => kinds.includes(entry.kind));
  const queued = entries.reduce((total, entry) => total + entry.queued, 0);
  const running = entries.reduce((total, entry) => total + entry.running, 0);
  const cancelling = entries.reduce((total, entry) => total + entry.cancelling, 0);
  const retained = entries.reduce((total, entry) => total + entry.retained, 0);
  const active = queued + running + cancelling;
  return {
    id,
    title,
    state: active ? 'busy' : retained ? 'ready' : 'idle',
    summary: active ? `${active} 个后端任务仍在执行` : retained ? `已保留 ${retained} 个任务结果` : '当前无任务',
    facts: [
      { label: '排队 / 执行 / 停止中', value: `${queued} / ${running} / ${cancelling}` },
      { label: '并发预算', value: budget },
      { label: '页面关闭', value: '任务继续；重新打开恢复状态与结果' },
      { label: '配置切换 / 内核重启', value: '活动任务失效并停止接受旧结果' },
      { label: '已提交请求的取消', value: '持有预算直到内核请求返回或超时' },
    ],
  };
}

export const dnsToolDiagnostics: ModuleSource[] = [{
  id: 'dns',
  title: 'DNS 与 Fake-IP',
  read: async () => {
    const runtime = await getToolRuntimeSnapshot();
    return status(
      runtime,
      'dns',
      'DNS 与 Fake-IP',
      ['dns_lookup', 'dns_cache', 'fake_ip_lookup', 'fake_ip_clear'],
      `查询 ${runtime.dnsMaxConcurrency} 路；清理 ${runtime.mutationMaxConcurrency} 路`,
    );
  },
}];

export const routeToolDiagnostics: ModuleSource[] = [{
  id: 'route-trace',
  title: '路由追踪',
  read: async () => {
    const runtime = await getToolRuntimeSnapshot();
    return status(runtime, 'route-trace', '路由追踪', ['route_trace'], `${runtime.routeMaxConcurrency} 路`);
  },
}];
