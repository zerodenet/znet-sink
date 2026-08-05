use serde::{Deserialize, Serialize};

use super::domain::{ClientScope, NodeId, PolicyId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelProbeKind {
    Outbound,
    ManualPolicy,
    ScheduledPolicyObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelProbeRequest {
    pub scope: ClientScope,
    pub kind: KernelProbeKind,
    pub node_id: Option<NodeId>,
    pub policy_id: Option<PolicyId>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelProbeResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelNodeSnapshot {
    pub node_tags: Vec<String>,
    pub policy_tags: Vec<String>,
}

/// Port consumed by the Client Core application layer. Implementations adapt
/// a concrete kernel protocol and contain any legacy compatibility inference.
/// It intentionally exposes no Tauri handle, command, event, or state type.
#[allow(async_fn_in_trait)]
pub trait ClientKernel {
    type Error;

    async fn node_snapshot(&self, scope: &ClientScope) -> Result<KernelNodeSnapshot, Self::Error>;

    async fn probe(&self, request: KernelProbeRequest) -> Result<KernelProbeResult, Self::Error>;
}
