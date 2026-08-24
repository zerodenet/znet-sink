use serde::{Deserialize, Serialize};

use super::dns_config::ClientDnsConfig;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub core: AppCoreConfig,
    #[serde(default)]
    pub logs: AppLogConfig,
    #[serde(default)]
    pub ui: AppUiConfig,
    #[serde(default)]
    pub local_proxy: AppLocalProxyConfig,
    #[serde(default)]
    pub tun: AppTunConfig,
    #[serde(default)]
    pub dns: AppDnsConfig,
    #[serde(default)]
    pub routing: AppRoutingConfig,
    #[serde(default)]
    pub url_test: AppUrlTestConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            core: AppCoreConfig::default(),
            logs: AppLogConfig::default(),
            ui: AppUiConfig::default(),
            local_proxy: AppLocalProxyConfig::default(),
            tun: AppTunConfig::default(),
            dns: AppDnsConfig::default(),
            routing: AppRoutingConfig::default(),
            url_test: AppUrlTestConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCoreConfig {
    #[serde(default = "default_kernel")]
    pub kernel: String,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    #[serde(default = "default_true")]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub cleanup_proxy_on_exit: bool,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub config_path: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub socket: Option<String>,
    #[serde(default = "default_network_probe_urls")]
    pub network_probe_urls: Vec<String>,
}

impl Default for AppCoreConfig {
    fn default() -> Self {
        Self {
            kernel: default_kernel(),
            auto_connect: true,
            auto_start: true,
            cleanup_proxy_on_exit: true,
            executable_path: None,
            download_url: None,
            config_path: None,
            working_dir: None,
            socket: None,
            network_probe_urls: default_network_probe_urls(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_max_entries")]
    pub max_entries: usize,
}

impl Default for AppLogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            max_entries: default_log_max_entries(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(default)]
    pub hidden_menu_keys: Vec<String>,
    #[serde(default = "default_true")]
    pub traffic_ball_enabled: bool,
    #[serde(default)]
    pub default_route: Option<String>,
}

impl Default for AppUiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            ui_mode: default_ui_mode(),
            sidebar_collapsed: false,
            hidden_menu_keys: vec!["debug".to_string()],
            traffic_ball_enabled: true,
            default_route: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLocalProxyConfig {
    #[serde(default = "default_local_proxy_host")]
    pub host: String,
    #[serde(default = "default_local_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub source_proxy_config_id: Option<String>,
    #[serde(default = "default_proxy_bypass")]
    pub bypass: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppTunConfig {
    /// Explicit ZNet-Sink desired state. `None` preserves the historical
    /// auto-connect behavior until the user first toggles TUN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_tun_addr")]
    pub addr: String,
    #[serde(default = "default_tun_mask")]
    pub mask: String,
    #[serde(default)]
    pub secondary_addr: Option<String>,
    #[serde(default = "default_tun_tag", deserialize_with = "deserialize_tun_tag")]
    pub tag: String,
    #[serde(default = "default_tun_mtu")]
    pub mtu: u16,
    #[serde(default = "default_true")]
    pub dual_stack: bool,
    #[serde(default)]
    pub dns_hijack: bool,
}

/// Global DNS/Fake-IP settings owned by ZNet-Sink and injected into the
/// effective Zero runtime configuration. This is intentionally independent
/// from any proxy profile so switching profiles cannot reset DNS behavior.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppDnsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<ClientDnsConfig>,
    #[serde(default)]
    pub dns_hijack: bool,
}

impl Default for AppDnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            config: None,
            dns_hijack: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppRoutingConfig {
    #[serde(default = "default_true")]
    pub inject_common_rules: bool,
}

impl Default for AppRoutingConfig {
    fn default() -> Self {
        Self {
            inject_common_rules: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUrlTestConfig {
    #[serde(default = "default_url_test_tolerance_ms")]
    pub tolerance_ms: u64,
}

impl Default for AppUrlTestConfig {
    fn default() -> Self {
        Self {
            tolerance_ms: default_url_test_tolerance_ms(),
        }
    }
}

impl Default for AppTunConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            name: None,
            addr: default_tun_addr(),
            mask: default_tun_mask(),
            secondary_addr: None,
            tag: default_tun_tag(),
            mtu: default_tun_mtu(),
            dual_stack: true,
            dns_hijack: false,
        }
    }
}

impl Default for AppLocalProxyConfig {
    fn default() -> Self {
        Self {
            host: default_local_proxy_host(),
            port: default_local_proxy_port(),
            source_proxy_config_id: None,
            bypass: default_proxy_bypass(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigPatch {
    pub core: Option<AppCoreConfigPatch>,
    pub logs: Option<AppLogConfigPatch>,
    pub ui: Option<AppUiConfigPatch>,
    pub local_proxy: Option<AppLocalProxyConfigPatch>,
    pub tun: Option<AppTunConfigPatch>,
    pub dns: Option<AppDnsConfigPatch>,
    pub routing: Option<AppRoutingConfigPatch>,
    pub url_test: Option<AppUrlTestConfigPatch>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCoreConfigPatch {
    pub kernel: Option<String>,
    pub auto_connect: Option<bool>,
    pub auto_start: Option<bool>,
    pub cleanup_proxy_on_exit: Option<bool>,
    pub executable_path: Option<Option<String>>,
    pub download_url: Option<Option<String>>,
    pub config_path: Option<Option<String>>,
    pub working_dir: Option<Option<String>>,
    pub socket: Option<Option<String>>,
    pub network_probe_urls: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLogConfigPatch {
    pub level: Option<String>,
    pub max_entries: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUiConfigPatch {
    pub theme: Option<String>,
    pub ui_mode: Option<String>,
    pub sidebar_collapsed: Option<bool>,
    pub hidden_menu_keys: Option<Vec<String>>,
    pub traffic_ball_enabled: Option<bool>,
    pub default_route: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLocalProxyConfigPatch {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub source_proxy_config_id: Option<Option<String>>,
    pub bypass: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTunConfigPatch {
    pub enabled: Option<bool>,
    pub name: Option<Option<String>>,
    pub addr: Option<String>,
    pub mask: Option<String>,
    pub secondary_addr: Option<Option<String>>,
    pub tag: Option<String>,
    pub mtu: Option<u16>,
    pub dual_stack: Option<bool>,
    pub dns_hijack: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDnsConfigPatch {
    pub enabled: Option<bool>,
    pub config: Option<Option<ClientDnsConfig>>,
    pub dns_hijack: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRoutingConfigPatch {
    pub inject_common_rules: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUrlTestConfigPatch {
    pub tolerance_ms: Option<u64>,
}

fn default_schema_version() -> String {
    "gui.app.v1".to_string()
}

fn default_kernel() -> String {
    "zero".to_string()
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_ui_mode() -> String {
    "lite".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_max_entries() -> usize {
    500
}

fn default_local_proxy_host() -> String {
    "127.0.0.1".to_string()
}

fn default_local_proxy_port() -> u16 {
    7890
}

fn default_tun_addr() -> String {
    "10.66.0.1/30".to_string()
}

fn default_tun_mask() -> String {
    "255.255.255.252".to_string()
}

fn default_tun_tag() -> String {
    "znet-sink-tun".to_string()
}

fn deserialize_tun_tag<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let tag = String::deserialize(deserializer)?;
    // `proxy` was the original ZNet-Sink default introduced with command-managed
    // TUN. Treat only that exact legacy value as a migration sentinel so real
    // user-defined inbound tags remain untouched.
    if tag == "proxy" {
        Ok(default_tun_tag())
    } else {
        Ok(tag)
    }
}

fn default_tun_mtu() -> u16 {
    1500
}

pub fn default_url_test_tolerance_ms() -> u64 {
    50
}

pub fn default_proxy_bypass() -> Vec<String> {
    [
        "<local>",
        "localhost",
        "127.*",
        "[::1]",
        "10.*",
        "192.168.*",
        "172.16.*",
        "172.17.*",
        "172.18.*",
        "172.19.*",
        "172.20.*",
        "172.21.*",
        "172.22.*",
        "172.23.*",
        "172.24.*",
        "172.25.*",
        "172.26.*",
        "172.27.*",
        "172.28.*",
        "172.29.*",
        "172.30.*",
        "172.31.*",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn default_network_probe_urls() -> Vec<String> {
    vec![
        "http://ip-api.com/json/?fields=query,country,regionName,city,org,isp".to_string(),
        "https://ipinfo.io/json".to_string(),
        "https://httpbin.org/ip".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use serde_json::json;

    #[test]
    fn common_rule_injection_defaults_to_enabled_for_new_and_legacy_configs() {
        let new_config = AppConfig::default();
        assert!(new_config.routing.inject_common_rules);

        let missing_routing: AppConfig = serde_json::from_value(json!({})).unwrap();
        assert!(missing_routing.routing.inject_common_rules);

        let legacy_empty_routing: AppConfig =
            serde_json::from_value(json!({ "routing": {} })).unwrap();
        assert!(legacy_empty_routing.routing.inject_common_rules);
    }

    #[test]
    fn common_rule_injection_preserves_an_explicit_disabled_choice() {
        let config: AppConfig = serde_json::from_value(json!({
            "routing": { "injectCommonRules": false }
        }))
        .unwrap();

        assert!(!config.routing.inject_common_rules);
    }

    #[test]
    fn traffic_ball_defaults_to_enabled_and_preserves_opt_out() {
        assert!(AppConfig::default().ui.traffic_ball_enabled);

        let legacy: AppConfig = serde_json::from_value(json!({ "ui": {} })).unwrap();
        assert!(legacy.ui.traffic_ball_enabled);

        let disabled: AppConfig = serde_json::from_value(json!({
            "ui": { "trafficBallEnabled": false }
        }))
        .unwrap();
        assert!(!disabled.ui.traffic_ball_enabled);
    }

    #[test]
    fn urltest_tolerance_defaults_to_50ms_for_new_and_legacy_configs() {
        assert_eq!(AppConfig::default().url_test.tolerance_ms, 50);

        let legacy: AppConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(legacy.url_test.tolerance_ms, 50);
    }

    #[test]
    fn urltest_tolerance_preserves_explicit_zero() {
        let config: AppConfig = serde_json::from_value(json!({
            "urlTest": { "toleranceMs": 0 }
        }))
        .unwrap();

        assert_eq!(config.url_test.tolerance_ms, 0);
    }

    #[test]
    fn tun_defaults_use_a_narrow_subnet_and_keep_dns_hijack_opt_in() {
        let config: AppConfig = serde_json::from_value(json!({})).unwrap();
        assert!(config.tun.enabled.is_none());
        assert_eq!(config.tun.addr, "10.66.0.1/30");
        assert_eq!(config.tun.mask, "255.255.255.252");
        assert_eq!(config.tun.tag, "znet-sink-tun");
        assert!(config.tun.secondary_addr.is_none());
        assert!(config.tun.dual_stack);
        assert!(!config.tun.dns_hijack);
    }

    #[test]
    fn tun_desired_state_round_trips_when_explicit() {
        let enabled: AppConfig = serde_json::from_value(json!({
            "tun": { "enabled": true }
        }))
        .unwrap();
        assert_eq!(enabled.tun.enabled, Some(true));

        let disabled: AppConfig = serde_json::from_value(json!({
            "tun": { "enabled": false }
        }))
        .unwrap();
        assert_eq!(disabled.tun.enabled, Some(false));
    }

    #[test]
    fn global_dns_defaults_to_disabled_for_legacy_configs() {
        let config: AppConfig = serde_json::from_value(json!({})).unwrap();
        assert!(!config.dns.enabled);
        assert!(config.dns.config.is_none());
        assert!(!config.dns.dns_hijack);
    }

    #[test]
    fn legacy_proxy_tun_tag_migrates_without_overwriting_custom_tags() {
        let legacy: AppConfig = serde_json::from_value(json!({
            "tun": { "tag": "proxy" }
        }))
        .unwrap();
        assert_eq!(legacy.tun.tag, "znet-sink-tun");

        let custom: AppConfig = serde_json::from_value(json!({
            "tun": { "tag": "custom-tun-in" }
        }))
        .unwrap();
        assert_eq!(custom.tun.tag, "custom-tun-in");
    }
}
