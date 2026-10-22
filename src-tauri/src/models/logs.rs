use serde::{Deserialize, Serialize};
use serde_json::Value;
#[path = "logs/diagnostic.rs"]
mod diagnostic;
pub(crate) use diagnostic::{
    HostDiagnosticEvent, HostOutcome, NodeSnapshotEvent, PluginAction, ProbeEvent, ProcessEvent,
    RuleDependencyEvent, ScheduleEvent,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSource {
    App,
    Core,
    Plugin,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub source: LogSource,
    pub level: LogLevel,
    pub message: String,
    pub fields: Option<Value>,
    pub occurred_at_unix_ms: u64,
    /// Host-selected closed classification, never accepted by LogAppend. Private
    /// stored history is not authenticated evidence for current runtime health.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "diagnostic::deserialize_optional"
    )]
    pub(crate) diagnostic_event: Option<HostDiagnosticEvent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogAppend {
    pub source: LogSource,
    pub level: LogLevel,
    pub message: String,
    pub fields: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub source: Option<LogSource>,
    pub level: Option<LogLevel>,
    pub min_level: Option<LogLevel>,
    pub limit: Option<usize>,
    pub before_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub items: Vec<LogEntry>,
    pub has_more: bool,
    pub oldest_available_id: Option<u64>,
}
