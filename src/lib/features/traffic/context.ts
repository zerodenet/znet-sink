import { getContext, setContext, onDestroy, onMount } from 'svelte';
import { trafficGateway } from '$lib/features/traffic/client';
import { TrafficWorkspace } from './workspace';

const key = Symbol('traffic-workspace');
export function provideTrafficWorkspace() {
  const workspace = new TrafficWorkspace(trafficGateway);
  setContext(key, workspace);
  onMount(() => {
    const synchronizeVisibility = () => workspace.session.setVisible(document.visibilityState !== 'hidden');
    synchronizeVisibility();
    document.addEventListener('visibilitychange', synchronizeVisibility);
    return () => document.removeEventListener('visibilitychange', synchronizeVisibility);
  });
  onDestroy(() => workspace.dispose());
}
export function useTrafficWorkspace(): TrafficWorkspace {
  const workspace = getContext<TrafficWorkspace | undefined>(key);
  if (!workspace) throw new Error('缺少客户端统计工作区');
  return workspace;
}
