use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSource {
    App,
    Core,
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
