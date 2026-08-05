use serde::{Deserialize, Serialize};

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

numeric_id!(SnapshotRevision);
numeric_id!(ConfigRevision);
numeric_id!(CoreInstanceId);
numeric_id!(ProbeJobId);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(pub String);

/// The complete scope needed to reject results from an older profile,
/// configuration, or kernel process.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientScope {
    pub profile_id: Option<ProfileId>,
    pub config_revision: ConfigRevision,
    pub core_instance_id: CoreInstanceId,
}

/// A node identity is intentionally wider than its kernel tag. Two profiles
/// may use the same tag without sharing observations or history.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeId {
    pub profile_id: ProfileId,
    pub config_revision: ConfigRevision,
    pub tag: String,
}

/// A policy identity is scoped in the same way as a node identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyId {
    pub profile_id: ProfileId,
    pub config_revision: ConfigRevision,
    pub tag: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    #[default]
    Initializing,
    Ready,
    Degraded,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeJobKind {
    Outbound,
    ManualPolicy,
    ScheduledPolicyObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeObservationSource {
    ManualOutbound,
    ManualPolicy,
    ScheduledPolicy,
}

impl ProbeJobKind {
    pub fn observation_source(self) -> ProbeObservationSource {
        match self {
            Self::Outbound => ProbeObservationSource::ManualOutbound,
            Self::ManualPolicy => ProbeObservationSource::ManualPolicy,
            Self::ScheduledPolicyObservation => ProbeObservationSource::ScheduledPolicy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeJobState {
    Running,
    Completed,
    PartiallyFailed,
    Failed,
    TimedOut,
    Cancelled,
    InvalidatedByConfigChange,
    InvalidatedByCoreRestart,
}

impl ProbeJobState {
    pub fn is_terminal(self) -> bool {
        self != Self::Running
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeTargetResult {
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
    pub source: ProbeObservationSource,
    pub observed_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeObservation {
    pub scope: ClientScope,
    pub job_kind: ProbeJobKind,
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
    pub source: ProbeObservationSource,
    pub observed_at_unix_ms: u64,
    pub selected_tag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeJobSnapshot {
    pub id: ProbeJobId,
    pub scope: ClientScope,
    pub kind: ProbeJobKind,
    pub state: ProbeJobState,
    pub target_tags: Vec<String>,
    pub results: Vec<ProbeTargetResult>,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub started_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub deadline_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartProbeRequest {
    pub kind: ProbeJobKind,
    pub target_tags: Vec<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartProbeOutcome {
    pub job: ProbeJobSnapshot,
    pub created: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCoreError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeObservationSource {
    RuntimeSnapshot,
    ManualOutbound,
    ManualPolicy,
    ScheduledPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub tag: String,
    pub protocol: String,
    pub server: Option<String>,
    pub port: Option<u64>,
    pub udp: Option<bool>,
    pub network: Option<String>,
    pub tls: Option<bool>,
    pub sni: Option<String>,
    pub cipher: Option<String>,
    pub group_tags: Vec<String>,
    pub selected_in: Vec<String>,
    pub runtime_available: bool,
    pub alive: Option<bool>,
    pub latency_ms: Option<u64>,
    pub last_observed_at_unix_ms: Option<u64>,
    pub last_observation_source: Option<NodeObservationSource>,
    pub active_probe_job_ids: Vec<ProbeJobId>,
    pub history: Vec<ProbeObservation>,
    pub action_valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeGroupSnapshot {
    pub id: PolicyId,
    pub tag: String,
    pub kind: String,
    pub selected: Option<String>,
    pub member_tags: Vec<String>,
    pub runtime_available: bool,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeScreenSnapshot {
    pub revision: SnapshotRevision,
    pub scope: ClientScope,
    pub source_status: SourceStatus,
    pub groups: Vec<NodeGroupSnapshot>,
    pub nodes: Vec<NodeSnapshot>,
    pub active_probe_jobs: Vec<ProbeJobSnapshot>,
}

/// Smallest authoritative snapshot introduced by phase one. Later node and
/// probe projections extend this contract instead of creating new page-owned
/// sources of truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCoreSnapshot {
    pub revision: SnapshotRevision,
    pub scope: ClientScope,
    pub source_status: SourceStatus,
    pub active_probe_jobs: Vec<ProbeJobSnapshot>,
}

impl ClientScope {
    pub fn node_id(&self, tag: impl Into<String>) -> Option<NodeId> {
        self.profile_id.clone().map(|profile_id| NodeId {
            profile_id,
            config_revision: self.config_revision,
            tag: tag.into(),
        })
    }

    pub fn policy_id(&self, tag: impl Into<String>) -> Option<PolicyId> {
        self.profile_id.clone().map(|profile_id| PolicyId {
            profile_id,
            config_revision: self.config_revision,
            tag: tag.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientCoreSnapshot, ClientScope, ConfigRevision, CoreInstanceId, ProfileId,
        SnapshotRevision, SourceStatus,
    };
    use serde_json::json;

    #[test]
    fn snapshot_contract_serializes_for_the_tauri_bridge() {
        let value = serde_json::to_value(ClientCoreSnapshot {
            revision: SnapshotRevision(7),
            scope: ClientScope {
                profile_id: Some(ProfileId("profile-a".to_string())),
                config_revision: ConfigRevision(3),
                core_instance_id: CoreInstanceId(2),
            },
            source_status: SourceStatus::Ready,
            active_probe_jobs: Vec::new(),
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "revision": 7,
                "scope": {
                    "profileId": "profile-a",
                    "configRevision": 3,
                    "coreInstanceId": 2
                },
                "sourceStatus": "ready",
                "activeProbeJobs": []
            })
        );
    }
}
