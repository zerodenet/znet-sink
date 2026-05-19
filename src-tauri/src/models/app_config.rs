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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            core: AppCoreConfig::default(),
            logs: AppLogConfig::default(),
            ui: AppUiConfig::default(),
            local_proxy: AppLocalProxyConfig::default(),
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
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub config_path: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub socket: Option<String>,
}

impl Default for AppCoreConfig {
    fn default() -> Self {
        Self {
            kernel: default_kernel(),
            auto_connect: true,
            auto_start: false,
            executable_path: None,
            config_path: None,
            working_dir: None,
            socket: None,
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
            sidebar_collapsed: false,
            hidden_menu_keys: Vec::new(),
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
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCoreConfigPatch {
    pub kernel: Option<String>,
    pub auto_connect: Option<bool>,
    pub auto_start: Option<bool>,
    pub executable_path: Option<Option<String>>,
    pub config_path: Option<Option<String>>,
    pub working_dir: Option<Option<String>>,
    pub socket: Option<Option<String>>,
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

fn default_schema_version() -> String {
    "gui.app.v1".to_string()
}

fn default_kernel() -> String {
    "zero".to_string()
}

fn default_true() -> bool {
    true
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
