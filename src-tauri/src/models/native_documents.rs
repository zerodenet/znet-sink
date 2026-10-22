use serde::{Deserialize, Serialize};
use tauri_plugin_znet_native_io::ReceiptStatus;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentPurpose {
    Configuration,
    Settings,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentDelivery {
    Save,
    Share,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCapabilities {
    pub platform: &'static str,
    pub available: bool,
    pub minimum_android_api: u32,
    pub adaptation_state: &'static str,
    pub reason: Option<&'static str>,
    pub import_maximum_bytes: u32,
    pub settings_maximum_bytes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReceipt {
    pub operation_id: String,
    pub status: ReceiptStatus,
    pub content: Option<String>,
    pub bytes_read: Option<u32>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticScope {
    pub profile_id: Option<String>,
    pub config_revision: String,
    pub client_instance_id: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOmission {
    pub category: String,
    pub count: Option<u64>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPreview {
    pub plan_id: String,
    pub created_at_unix_ms: String,
    pub expires_at_unix_ms: String,
    pub scope: DiagnosticScope,
    pub bytes: u32,
    pub log_count: usize,
    pub frame_count: Option<usize>,
    pub task_count: usize,
    pub omissions: Vec<DiagnosticOmission>,
}
