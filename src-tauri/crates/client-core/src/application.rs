use super::domain::{
    ClientCoreSnapshot, ClientScope, ConfigRevision, CoreInstanceId, ProbeObservation, ProfileId,
    SnapshotRevision, SourceStatus,
};

const MAX_RETAINED_OBSERVATIONS: usize = 10_000;
const MAX_OBSERVATIONS_PER_TARGET: usize = 20;

/// Revisioned state container for authoritative client workflows.
///
/// Mutations are intentionally expressed as lifecycle facts. Platform code
/// calls these only after a configuration transition or kernel start commits.
#[derive(Debug)]
pub struct ClientCore {
    snapshot: ClientCoreSnapshot,
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
            observations: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> ClientCoreSnapshot {
        self.snapshot.clone()
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

    /// Records at most one result for each requested target. Results from an
    /// older scope or a terminal job are deterministically ignored.

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

    /// Commit a new active configuration generation. This is also used when
    /// the active profile remains the same but its content changes.
    pub fn configuration_committed(
        &mut self,
        active_profile_id: Option<ProfileId>,
        config_revision: ConfigRevision,
        _now_unix_ms: u64,
    ) {
        self.snapshot.scope.profile_id = active_profile_id;
        self.snapshot.scope.config_revision = config_revision;
        self.advance_snapshot();
    }

    /// Commit a newly running kernel instance. Restarts and watchdog recovery
    /// both create a new generation, making old asynchronous results stale.
    pub fn core_instance_started(&mut self, _now_unix_ms: u64) {
        self.snapshot.scope.core_instance_id.0 =
            self.snapshot.scope.core_instance_id.0.saturating_add(1);
        self.snapshot.source_status = SourceStatus::Ready;
        self.advance_snapshot();
    }

    /// Mark the current kernel generation unavailable immediately. A later
    /// successful start creates the next generation, but jobs are invalidated
    /// at disconnect time rather than drifting into an unrelated timeout.
    pub fn core_instance_lost(&mut self, _now_unix_ms: u64) {
        let status_changed = self.snapshot.source_status != SourceStatus::Offline;
        self.snapshot.source_status = SourceStatus::Offline;
        if status_changed {
            self.advance_snapshot();
        }
    }

    pub fn set_source_status(&mut self, status: SourceStatus) {
        if self.snapshot.source_status != status {
            self.snapshot.source_status = status;
            self.advance_snapshot();
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

    pub fn advance_snapshot(&mut self) {
        self.snapshot.revision.0 = self.snapshot.revision.0.saturating_add(1);
    }
}
