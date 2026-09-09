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
