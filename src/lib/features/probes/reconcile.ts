import type { ProbeJobSnapshot } from '$lib/types/gui-api';

export interface ProbeJobViewState {
  directJobs: Map<number, ProbeJobSnapshot>;
  terminalJobIds: Set<number>;
}

/**
 * Apply an authoritative probe-job snapshot without mutating the previous
 * state. Running jobs are rendered immediately; terminal snapshots tombstone
 * the job so delayed start responses and node-screen snapshots cannot
 * resurrect its spinner.
 */
export function applyProbeJobSnapshot(
  state: ProbeJobViewState,
  job: ProbeJobSnapshot,
): ProbeJobViewState {
  const directJobs = new Map(state.directJobs);
  const terminalJobIds = new Set(state.terminalJobIds);
  const current = directJobs.get(job.id);
  if (current && current.updatedAtUnixMs > job.updatedAtUnixMs) return state;

  if (job.state === 'running') {
    // Probe jobs have an irreversible lifecycle. A terminal update may arrive
    // before the original start command resolves for very fast failures, so a
    // later-delivered running snapshot must never reopen the job.
    if (terminalJobIds.has(job.id)) return state;
    directJobs.set(job.id, job);
  } else {
    directJobs.delete(job.id);
    terminalJobIds.add(job.id);
  }

  return { directJobs, terminalJobIds };
}

/** Merge recoverable backend jobs with immediate command/event state. */
export function mergeActiveProbeJobs(
  snapshotJobs: ProbeJobSnapshot[],
  directJobs: ReadonlyMap<number, ProbeJobSnapshot>,
  terminalJobIds: ReadonlySet<number>,
): ProbeJobSnapshot[] {
  const jobs = new Map<number, ProbeJobSnapshot>();
  for (const job of snapshotJobs) {
    if (job.state !== 'running' || terminalJobIds.has(job.id)) continue;
    jobs.set(job.id, job);
  }
  for (const job of directJobs.values()) {
    if (job.state !== 'running' || terminalJobIds.has(job.id)) continue;
    const current = jobs.get(job.id);
    if (!current || current.updatedAtUnixMs <= job.updatedAtUnixMs) {
      jobs.set(job.id, job);
    }
  }
  return [...jobs.values()].sort((left, right) => left.id - right.id);
}

export function shouldApplyNodeScreenSnapshot(input: {
  currentRevision?: number;
  candidateRevision: number;
  requestSequence: number;
  lastAppliedRequest: number;
}): boolean {
  if (input.requestSequence < input.lastAppliedRequest) return false;
  return input.currentRevision === undefined || input.candidateRevision >= input.currentRevision;
}
