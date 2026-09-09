import { invoke } from '@tauri-apps/api/core';
import type { StartProbeRequest, ProbeJobSnapshot } from '$lib/types/gui-api';
export async function startProbeJob(request: StartProbeRequest): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_start', { request });
}

export async function getProbeJob(jobId: number): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_get', { jobId });
}

export async function listProbeJobs(profileId?: string): Promise<ProbeJobSnapshot[]> {
  return invoke('gui_probe_job_list', { profileId });
}

export async function cancelProbeJob(jobId: number): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_cancel', { jobId });
}

export interface ProbeRuntimeSnapshot {
  maxConcurrency: number; maxPendingTargets: number;
  queued: number; inFlight: number; draining: number; jobs: number;
  kernelCancellation: 'unsupported';
}
export async function getProbeRuntimeSnapshot(): Promise<ProbeRuntimeSnapshot> {
  return invoke('gui_probe_runtime_snapshot');
}
