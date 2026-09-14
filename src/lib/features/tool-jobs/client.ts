import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  StartToolJobRequest,
  ToolJobKind,
  ToolJobSnapshot,
  ToolRuntimeSnapshot,
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

export function cancelToolJob(jobId: number): Promise<ToolJobSnapshot> {
  return invoke('gui_tool_job_cancel', { jobId });
}

export function getToolRuntimeSnapshot(): Promise<ToolRuntimeSnapshot> {
  return invoke('gui_tool_runtime_snapshot');
}

export function subscribeToolJobs(
  handler: (job: ToolJobSnapshot) => void,
): Promise<() => void> {
  return listen<ToolJobSnapshot>('client-core:tool-job-updated', (event) => handler(event.payload));
}
