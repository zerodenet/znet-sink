use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTunFamilyEgress {
    pub availability: String,
    pub interface: Option<String>,
    pub reason: Option<String>,
}

impl Default for GuiTunFamilyEgress {
    fn default() -> Self {
        Self {
            availability: "unknown".to_string(),
            interface: None,
            reason: None,
        }
    }
}

/// Detailed TUN runtime state returned by Zero v0.0.16 dev builds.
///
/// The compatibility fields (`supported`, `enabled`, `state`, `reason`) keep
/// existing GUI state transitions working while the remaining fields expose
/// Zero's authoritative route and interface state.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTunStatus {
    pub key: String,
    pub supported: bool,
    pub enabled: bool,
    pub state: String,
    pub reason: Option<String>,
    pub name: Option<String>,
    pub addr: Option<String>,
    pub addresses: Vec<String>,
    pub mtu: Option<u16>,
    pub tag: Option<String>,
    pub healthy: bool,
    pub auto_route: bool,
    pub include_cidrs: Option<Vec<String>>,
    pub exclude_cidrs: Option<Vec<String>>,
    pub dual_stack: bool,
    pub strict_route: bool,
    pub dns_hijack: bool,
    pub fake_ip_enabled: bool,
    pub dns_hijacked_queries: u64,
    pub egress_interface: Option<String>,
    pub egress_interface_v4: Option<String>,
    pub egress_interface_v6: Option<String>,
    pub ipv4_egress: GuiTunFamilyEgress,
    pub ipv6_egress: GuiTunFamilyEgress,
    pub network_generation: u64,
    pub address_family_policy: Option<String>,
    pub ipv6_to_ipv4_fallbacks: u64,
    pub last_error: Option<String>,
    pub managed_by_config: bool,
}
