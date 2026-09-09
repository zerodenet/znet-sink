use crate::{
    ClientCoreError, ClientScope, ProbeJobId, ProbeJobKind, ProbeJobSnapshot, ProbeJobState,
    ProbeObservation, ProbeTargetResult, ProfileId, StartProbeOutcome, StartProbeRequest,
};
use std::collections::{BTreeMap, HashSet};
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const MIN_PROBE_TIMEOUT_MS: u64 = 1_000;
const MAX_PROBE_TIMEOUT_MS: u64 = 600_000;
const MAX_RETAINED_PROBE_JOBS: usize = 256;
/// Tool-owned job state machine. The host supplies scope facts and consumes observations.
#[derive(Debug, Default)]
pub struct ProbeJobs {
    probe_jobs: BTreeMap<ProbeJobId, ProbeJobSnapshot>,
    next_probe_job_id: u64,
    observations: Vec<ProbeObservation>,
}
impl ProbeJobs {
    pub fn take_observations(&mut self) -> Vec<ProbeObservation> {
        std::mem::take(&mut self.observations)
    }
    pub fn get_probe_job(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        self.probe_jobs.get(&id).cloned()
    }

    pub fn list_probe_jobs(&self, profile_id: Option<&ProfileId>) -> Vec<ProbeJobSnapshot> {
        self.probe_jobs
            .values()
            .filter(|job| profile_id.is_none() || job.scope.profile_id.as_ref() == profile_id)
            .cloned()
            .collect()
    }

    pub fn start_probe(
        &mut self,
        scope: &ClientScope,
        request: StartProbeRequest,
        now_unix_ms: u64,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        if scope.profile_id.is_none() {
            return Err(ClientCoreError {
                code: "active_profile_required".to_string(),
                message: "an active profile is required before probing".to_string(),
            });
        }

        let target_tags = normalize_target_tags(request.target_tags);
        if target_tags.is_empty() {
            return Err(ClientCoreError {
                code: "probe_targets_required".to_string(),
                message: "at least one non-empty probe target is required".to_string(),
            });
        }

        if let Some(job) = self.probe_jobs.values().find(|job| {
            !job.state.is_terminal()
                && &job.scope == scope
                && job.kind == request.kind
                && job.target_tags == target_tags
        }) {
            return Ok(StartProbeOutcome {
                job: job.clone(),
                created: false,
            });
        }

        self.next_probe_job_id = self.next_probe_job_id.saturating_add(1);
        let id = ProbeJobId(self.next_probe_job_id);
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(DEFAULT_PROBE_TIMEOUT_MS)
            .clamp(MIN_PROBE_TIMEOUT_MS, MAX_PROBE_TIMEOUT_MS);
        let job = ProbeJobSnapshot {
            id,
            scope: scope.clone(),
            kind: request.kind,
            state: ProbeJobState::Running,
            target_tags,
            results: Vec::new(),
            completed: 0,
            succeeded: 0,
            failed: 0,
            started_at_unix_ms: now_unix_ms,
            updated_at_unix_ms: now_unix_ms,
            deadline_at_unix_ms: now_unix_ms.saturating_add(timeout_ms),
        };
        self.probe_jobs.insert(id, job.clone());
        self.prune_probe_jobs();

        Ok(StartProbeOutcome { job, created: true })
    }

    pub fn record_probe_result(
        &mut self,
        id: ProbeJobId,
        expected_scope: &ClientScope,
        result: ProbeTargetResult,
    ) -> Option<ProbeJobSnapshot> {
        let (updated, observation) = {
            let job = self.probe_jobs.get_mut(&id)?;
            if job.state.is_terminal() || &job.scope != expected_scope {
                return None;
            }
            if result.source != job.kind.observation_source() {
                return None;
            }
            if !job.target_tags.contains(&result.target_tag)
                || job
                    .results
                    .iter()
                    .any(|existing| existing.target_tag == result.target_tag)
            {
                return None;
            }

            job.updated_at_unix_ms = result.observed_at_unix_ms;
            if result.reachable {
                job.succeeded = job.succeeded.saturating_add(1);
            } else {
                job.failed = job.failed.saturating_add(1);
            }
            let observation = ProbeObservation {
                scope: job.scope.clone(),
                job_kind: job.kind,
                target_tag: result.target_tag.clone(),
                reachable: result.reachable,
                latency_ms: result.latency_ms,
                message: result.message.clone(),
                source: result.source,
                observed_at_unix_ms: result.observed_at_unix_ms,
                policy_tag: (job.kind == ProbeJobKind::ManualPolicy)
                    .then(|| result.target_tag.clone()),
                selected_tag: None,
            };
            job.results.push(result);
            job.completed = job.results.len();
            if job.completed == job.target_tags.len() {
                job.state = match (job.succeeded, job.failed) {
                    (_, 0) => ProbeJobState::Completed,
                    (0, _) => ProbeJobState::Failed,
                    _ => ProbeJobState::PartiallyFailed,
                };
            }
            (job.clone(), observation)
        };
        self.observations.push(observation);

        Some(updated)
    }

    pub fn cancel_probe(&mut self, id: ProbeJobId, now_unix_ms: u64) -> Option<ProbeJobSnapshot> {
        self.finish_probe(id, ProbeJobState::Cancelled, now_unix_ms)
    }

    pub fn timeout_probe(&mut self, id: ProbeJobId, now_unix_ms: u64) -> Option<ProbeJobSnapshot> {
        let pending_observations = self.probe_jobs.get(&id).and_then(|job| {
            (!job.state.is_terminal()).then(|| {
                job.target_tags
                    .iter()
                    .filter(|target| {
                        !job.results
                            .iter()
                            .any(|result| &result.target_tag == *target)
                    })
                    .map(|target| ProbeObservation {
                        scope: job.scope.clone(),
                        job_kind: job.kind,
                        target_tag: target.clone(),
                        reachable: false,
                        latency_ms: None,
                        message: Some("probe job timed out".to_string()),
                        source: match job.kind {
                            crate::ProbeJobKind::Outbound => {
                                crate::ProbeObservationSource::ManualOutbound
                            }
                            crate::ProbeJobKind::ManualPolicy => {
                                crate::ProbeObservationSource::ManualPolicy
                            }
                            crate::ProbeJobKind::ScheduledPolicyObservation => {
                                crate::ProbeObservationSource::ScheduledPolicy
                            }
                        },
                        observed_at_unix_ms: now_unix_ms,
                        policy_tag: (job.kind == ProbeJobKind::ManualPolicy)
                            .then(|| target.clone()),
                        selected_tag: None,
                    })
                    .collect::<Vec<_>>()
            })
        });
        let updated = self.finish_probe(id, ProbeJobState::TimedOut, now_unix_ms);
        if let Some(observations) = pending_observations {
            self.observations.extend(observations);
        }
        updated
    }

    fn finish_probe(
        &mut self,
        id: ProbeJobId,
        state: ProbeJobState,
        now_unix_ms: u64,
    ) -> Option<ProbeJobSnapshot> {
        let updated = {
            let job = self.probe_jobs.get_mut(&id)?;
            if job.state.is_terminal() {
                return Some(job.clone());
            }
            job.state = state;
            job.updated_at_unix_ms = now_unix_ms;
            job.clone()
        };

        Some(updated)
    }

    pub fn invalidate_active_jobs(&mut self, state: ProbeJobState, now_unix_ms: u64) -> bool {
        let mut changed = false;
        for job in self.probe_jobs.values_mut() {
            if !job.state.is_terminal() {
                job.state = state;
                job.updated_at_unix_ms = now_unix_ms;
                changed = true;
            }
        }
        changed
    }

    fn prune_probe_jobs(&mut self) {
        if self.probe_jobs.len() <= MAX_RETAINED_PROBE_JOBS {
            return;
        }
        let removable: Vec<_> = self
            .probe_jobs
            .iter()
            .filter_map(|(id, job)| job.state.is_terminal().then_some(*id))
            .take(self.probe_jobs.len() - MAX_RETAINED_PROBE_JOBS)
            .collect();
        for id in removable {
            self.probe_jobs.remove(&id);
        }
    }
}

fn normalize_target_tags(target_tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    target_tags
        .into_iter()
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.clone()))
        .collect()
}
