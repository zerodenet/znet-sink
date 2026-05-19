use serde::Serialize;

use crate::models::proxy_config::ProxyConfigCapabilities;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    pub key: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiCapabilitySnapshot {
    pub management: Vec<CapabilityItem>,
    pub proxy_features: Vec<CapabilityItem>,
    pub active_proxy_config_id: Option<String>,
    pub active_proxy_config_capabilities: ProxyConfigCapabilities,
}
