use serde::{Deserialize, Serialize};

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
    pub routing: AppRoutingConfig,
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
            routing: AppRoutingConfig::default(),
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
    #[serde(default)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppTunConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_tun_addr")]
    pub addr: String,
    #[serde(default = "default_tun_tag")]
    pub tag: String,
    #[serde(default = "default_tun_mtu")]
    pub mtu: u16,
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

impl Default for AppTunConfig {
    fn default() -> Self {
        Self {
            name: None,
            addr: default_tun_addr(),
            tag: default_tun_tag(),
            mtu: default_tun_mtu(),
        }
    }
}

impl Default for AppLocalProxyConfig {
    fn default() -> Self {
        Self {
            host: default_local_proxy_host(),
            port: default_local_proxy_port(),
            source_proxy_config_id: None,
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
    pub routing: Option<AppRoutingConfigPatch>,
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
    pub default_route: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLocalProxyConfigPatch {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub source_proxy_config_id: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTunConfigPatch {
    pub name: Option<Option<String>>,
    pub addr: Option<String>,
    pub tag: Option<String>,
    pub mtu: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRoutingConfigPatch {
    pub inject_common_rules: Option<bool>,
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
    "10.0.0.1/24".to_string()
}

fn default_tun_tag() -> String {
    "proxy".to_string()
}

fn default_tun_mtu() -> u16 {
    1500
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
}
