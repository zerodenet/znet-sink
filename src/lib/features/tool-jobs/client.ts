import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  StartToolJobRequest,
  ToolJobKind,
  ToolJobSnapshot,
  ToolRuntimeSnapshot,
  ClientCoreSnapshot,
  ClientScope,
} from '$lib/types/gui-api';

export function startToolJob(request: StartToolJobRequest): Promise<ToolJobSnapshot> {
  return invoke('gui_tool_job_start', { request });
}

export function listToolJobs(kind?: ToolJobKind): Promise<ToolJobSnapshot[]> {
  return invoke('gui_tool_job_list', { kind });
}

export function getToolJob(jobId: number): Promise<ToolJobSnapshot> {
  return invoke('gui_tool_job_get', { jobId });
}

export function cancelToolJob(jobId: number, expectedScope?: ClientScope, expectedClientRevision?: number): Promise<ToolJobSnapshot> {
  return invoke('gui_tool_job_cancel', { jobId, expectedScope, expectedClientRevision });
}

export function getToolScope(): Promise<ClientCoreSnapshot> {
  return invoke('gui_client_core_snapshot');
}

/** Page observation only; it neither owns nor resumes any backend task. */
export async function subscribeToolScope(handler: (snapshot: ClientCoreSnapshot | null) => void): Promise<() => void> {
  let disposed = false;
  let reading = false;
  let observation = 0;
  const stop = await listen<ClientCoreSnapshot>('client-core:updated', event => {
    observation++;
    if (!disposed) handler(event.payload);
  });
  const refresh = async () => {
    if (disposed || reading || document.hidden) return;
    reading = true;
    const observed = observation;
    try { const snapshot = await getToolScope(); if (!disposed && observed === observation) handler(snapshot); }
    catch { if (!disposed && observed === observation) handler(null); }
    finally { reading = false; }
  };
  const focused = () => { void refresh(); };
  const timer = setInterval(focused, 5_000);
  window.addEventListener('focus', focused);
  document.addEventListener('visibilitychange', focused);
  return () => { disposed = true; clearInterval(timer); stop(); window.removeEventListener('focus', focused); document.removeEventListener('visibilitychange', focused); };
}

export function getToolRuntimeSnapshot(): Promise<ToolRuntimeSnapshot> {
  return invoke('gui_tool_runtime_snapshot');
}

export function subscribeToolJobs(
  handler: (job: ToolJobSnapshot) => void,
): Promise<() => void> {
  return listen<ToolJobSnapshot>('client-core:tool-job-updated', (event) => handler(event.payload));
}
