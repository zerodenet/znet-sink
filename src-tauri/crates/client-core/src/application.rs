use std::collections::{BTreeMap, HashSet};

use super::domain::{
    ClientCoreError, ClientCoreSnapshot, ClientScope, ConfigRevision, CoreInstanceId, ProbeJobId,
    ProbeJobKind, ProbeJobSnapshot, ProbeJobState, ProbeObservation, ProbeTargetResult, ProfileId,
    SnapshotRevision, SourceStatus, StartProbeOutcome, StartProbeRequest,
};

const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const MIN_PROBE_TIMEOUT_MS: u64 = 1_000;
const MAX_PROBE_TIMEOUT_MS: u64 = 10 * 60_000;
const MAX_RETAINED_PROBE_JOBS: usize = 256;
const MAX_RETAINED_OBSERVATIONS: usize = 10_000;
const MAX_OBSERVATIONS_PER_TARGET: usize = 20;

/// Revisioned state container for authoritative client workflows.
///
/// Mutations are intentionally expressed as lifecycle facts. Platform code
/// calls these only after a configuration transition or kernel start commits.
#[derive(Debug)]
pub struct ClientCore {
    snapshot: ClientCoreSnapshot,
    probe_jobs: BTreeMap<ProbeJobId, ProbeJobSnapshot>,
    next_probe_job_id: u64,
    observations: Vec<ProbeObservation>,
}

impl ClientCore {
    pub fn new(active_profile_id: Option<ProfileId>, config_revision: ConfigRevision) -> Self {
        let has_profile = active_profile_id.is_some();
        Self {
            snapshot: ClientCoreSnapshot {
                revision: SnapshotRevision(1),
                scope: ClientScope {
                    profile_id: active_profile_id,
                    config_revision: if has_profile {
                        config_revision
                    } else {
                        ConfigRevision(0)
                    },
                    core_instance_id: CoreInstanceId(0),
                },
                source_status: SourceStatus::Initializing,
                active_probe_jobs: Vec::new(),
            },
            probe_jobs: BTreeMap::new(),
            next_probe_job_id: 0,
            observations: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> ClientCoreSnapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.active_probe_jobs = self
            .probe_jobs
            .values()
            .filter(|job| !job.state.is_terminal())
            .cloned()
            .collect();
        snapshot
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

    pub fn observations(&self, scope: Option<&ClientScope>) -> Vec<ProbeObservation> {
        self.observations
            .iter()
            .filter(|observation| scope.is_none() || Some(&observation.scope) == scope)
            .cloned()
            .collect()
    }

    pub fn observations_for_config(&self, scope: &ClientScope) -> Vec<ProbeObservation> {
        self.observations
            .iter()
            .filter(|observation| {
                observation.scope.profile_id == scope.profile_id
                    && observation.scope.config_revision == scope.config_revision
            })
            .cloned()
            .collect()
    }

    pub fn restore_observations(&mut self, observations: Vec<ProbeObservation>) {
        self.observations = observations
            .into_iter()
            .filter(|observation| observation.source == observation.job_kind.observation_source())
            .collect();
        self.prune_observations();
    }

    pub fn start_probe(
        &mut self,
        request: StartProbeRequest,
        now_unix_ms: u64,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        if self.snapshot.scope.profile_id.is_none() {
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
                && job.scope == self.snapshot.scope
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
            scope: self.snapshot.scope.clone(),
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
        self.advance_snapshot();
        Ok(StartProbeOutcome { job, created: true })
    }

    /// Records at most one result for each requested target. Results from an
    /// older scope or a terminal job are deterministically ignored.
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
        self.prune_observations();
        self.advance_snapshot();
        Some(updated)
    }

    pub fn record_observation(&mut self, observation: ProbeObservation) -> bool {
        if observation.scope != self.snapshot.scope
            || observation.source != observation.job_kind.observation_source()
        {
            return false;
        }
        if self.observations.iter().any(|existing| {
            existing.scope == observation.scope
                && existing.target_tag == observation.target_tag
                && existing.source == observation.source
                && existing.observed_at_unix_ms == observation.observed_at_unix_ms
                && existing.latency_ms == observation.latency_ms
                && existing.reachable == observation.reachable
        }) {
            return false;
        }
        self.observations.push(observation);
        self.prune_observations();
        self.advance_snapshot();
        true
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
            self.prune_observations();
        }
        updated
    }

    /// Commit a new active configuration generation. This is also used when
    /// the active profile remains the same but its content changes.
    pub fn configuration_committed(
        &mut self,
        active_profile_id: Option<ProfileId>,
        config_revision: ConfigRevision,
        now_unix_ms: u64,
    ) {
        self.invalidate_active_jobs(ProbeJobState::InvalidatedByConfigChange, now_unix_ms);
        self.snapshot.scope.profile_id = active_profile_id;
        self.snapshot.scope.config_revision = config_revision;
        self.advance_snapshot();
    }

    /// Commit a newly running kernel instance. Restarts and watchdog recovery
    /// both create a new generation, making old asynchronous results stale.
    pub fn core_instance_started(&mut self, now_unix_ms: u64) {
        self.invalidate_active_jobs(ProbeJobState::InvalidatedByCoreRestart, now_unix_ms);
        self.snapshot.scope.core_instance_id.0 =
            self.snapshot.scope.core_instance_id.0.saturating_add(1);
        self.snapshot.source_status = SourceStatus::Ready;
        self.advance_snapshot();
    }

    /// Mark the current kernel generation unavailable immediately. A later
    /// successful start creates the next generation, but jobs are invalidated
    /// at disconnect time rather than drifting into an unrelated timeout.
    pub fn core_instance_lost(&mut self, now_unix_ms: u64) {
        let jobs_changed =
            self.invalidate_active_jobs(ProbeJobState::InvalidatedByCoreRestart, now_unix_ms);
        let status_changed = self.snapshot.source_status != SourceStatus::Offline;
        self.snapshot.source_status = SourceStatus::Offline;
        if jobs_changed || status_changed {
            self.advance_snapshot();
        }
    }

    pub fn set_source_status(&mut self, status: SourceStatus) {
        if self.snapshot.source_status != status {
            self.snapshot.source_status = status;
            self.advance_snapshot();
        }
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
        self.advance_snapshot();
        Some(updated)
    }

    fn invalidate_active_jobs(&mut self, state: ProbeJobState, now_unix_ms: u64) -> bool {
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

    fn prune_observations(&mut self) {
        self.observations
            .sort_by_key(|observation| observation.observed_at_unix_ms);
        if self.observations.len() > MAX_RETAINED_OBSERVATIONS {
            self.observations
                .drain(..self.observations.len() - MAX_RETAINED_OBSERVATIONS);
        }

        // Enforce the per-target bound from newest to oldest.
        let mut counts = std::collections::HashMap::new();
        self.observations.reverse();
        self.observations.retain(|observation| {
            let key = (
                observation.scope.profile_id.clone(),
                observation.scope.config_revision,
                observation.target_tag.clone(),
            );
            let count = counts.entry(key).or_insert(0usize);
            *count += 1;
            *count <= MAX_OBSERVATIONS_PER_TARGET
        });
        self.observations.reverse();
    }

    fn advance_snapshot(&mut self) {
        self.snapshot.revision.0 = self.snapshot.revision.0.saturating_add(1);
    }
}

fn normalize_target_tags(target_tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    target_tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.clone()))
        .collect()
}
