use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub tag: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub exposure: &'static str,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Values {
    pub mtu: u16,
    pub mtu_origin: &'static str,
    pub udp_idle_seconds: String,
    pub udp_origin: &'static str,
    pub listener: Option<Listener>,
    pub listener_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialLeaf {
    pub tag: String,
    pub address_family: &'static str,
    pub interface: Option<String>,
    pub source_ip: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub source_updated_at_unix_ms: Option<String>,
    pub binding: Option<String>,
    pub read_only_reason: Option<String>,
    pub values: Option<Values>,
    pub edited_fields: Vec<String>,
    pub dial_leaves: Vec<DialLeaf>,
    pub pending_apply: bool,
    pub notice: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub profile_id: String,
    pub binding: String,
    pub changes: BTreeMap<String, Value>,
    pub reset: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub profile_id: String,
    pub before: Values,
    pub after: Values,
    pub changed_fields: Vec<String>,
    pub reset_fields: Vec<String>,
    pub lan_exposure_warning: bool,
    pub expires_at_unix_ms: String,
    pub state: &'static str,
    pub notice: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub plan_id: String,
    pub profile_id: String,
    pub state: &'static str,
    pub reason: String,
    pub snapshot: Option<Snapshot>,
}
