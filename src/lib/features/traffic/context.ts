import { getContext, setContext, onDestroy } from 'svelte';
import { trafficGateway } from '$lib/features/traffic/client';
import { TrafficWorkspace } from './workspace';

const key = Symbol('traffic-workspace');
export function provideTrafficWorkspace() {
  const workspace = new TrafficWorkspace(trafficGateway);
  setContext(key, workspace);
  onDestroy(() => workspace.dispose());
}
export function useTrafficWorkspace(): TrafficWorkspace {
  const workspace = getContext<TrafficWorkspace | undefined>(key);
  if (!workspace) throw new Error('缺少客户端统计工作区');
  return workspace;
}
