//! Portable transfer envelope. Network values retain the existing native settings schema.
use crate::client_core::ClientScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "znet.settings-transfer.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsDocument {
    pub schema_version: String,
    #[serde(default)]
    pub preferences: PortablePreferences,
    #[serde(default)]
    pub client_edits: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortablePreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_max_entries: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferExport {
    pub expected_revision: String,
    pub document: String,
    pub omitted_fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    pub plan_id: String,
    pub expected_revision: String,
    pub target_revision: String,
    pub scope: ClientScope,
    pub source_revision: Option<String>,
    pub changed_fields: Vec<String>,
    pub unchanged_fields: Vec<String>,
    pub expires_at_unix_ms: String,
    pub runtime_required: bool,
    pub compatibility_pending: bool,
    pub notice: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReceipt {
    pub plan_id: String,
    pub local_state: &'static str,
    pub runtime_state: &'static str,
    pub saved_revision: String,
    pub theme: String,
    pub notice: String,
}
