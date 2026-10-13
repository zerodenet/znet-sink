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
