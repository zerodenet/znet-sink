use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client_core::ClientScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolJobId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolJobKind {
    DnsLookup,
    DnsCache,
    FakeIpLookup,
    FakeIpClear,
    RouteTrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolJobState {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    InvalidatedByConfigChange,
    InvalidatedByCoreRestart,
}

impl ToolJobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::TimedOut
                | Self::Cancelled
                | Self::InvalidatedByConfigChange
                | Self::InvalidatedByCoreRestart
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartToolJobRequest {
    pub kind: ToolJobKind,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolJobError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolJobSnapshot {
    pub id: ToolJobId,
    pub scope: ClientScope,
    pub kind: ToolJobKind,
    pub state: ToolJobState,
    pub subject: String,
    pub params: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolJobError>,
    pub created_at_unix_ms: u64,
    pub started_at_unix_ms: Option<u64>,
    pub updated_at_unix_ms: u64,
    pub deadline_at_unix_ms: u64,
}
