use serde::Serialize;

use crate::models::core::CoreEndpoint;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfigSnapshot {
    pub kernel: String,
    pub auto_connect: bool,
    pub auto_start: bool,
    pub executable_path: Option<String>,
    pub executable_exists: bool,
    pub config_path: Option<String>,
    pub config_exists: Option<bool>,
    pub working_dir: Option<String>,
    pub working_dir_exists: Option<bool>,
    pub socket: Option<String>,
    pub endpoint: CoreEndpoint,
    pub launch_args: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreKernelInfo {
    pub kernel: String,
    pub executable_path: Option<String>,
    pub executable_exists: bool,
    pub file_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at_unix_ms: Option<u64>,
    pub recommended_install_dir: Option<String>,
    /// Whether an active proxy config exists (checked at call time).
    /// Connection and system-proxy actions require this; kernel management
    /// itself can still start a management-only runtime without one.
    pub has_active_config: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfigExportResult {
    pub proxy_config_id: String,
    pub path: String,
    pub app_config: CoreConfigSnapshot,
}

impl CoreConfigSnapshot {
    /// Only checks user-actionable preconditions: the executable binary must exist.
    /// Config file and working directory are auto-managed by the system
    /// (export_active, resolve_working_dir) — they are not blocking constraints.
    /// The "no active proxy config" guard lives in the self-test check
    /// (`check_active_proxy_config`), not here.
    pub fn validate_launchable(&self) -> Result<(), String> {
        let executable_path = self
            .executable_path
            .as_deref()
            .ok_or_else(|| "core executable path is not configured".to_string())?;
        if !self.executable_exists {
            return Err(format!("core executable does not exist: {executable_path}"));
        }
        Ok(())
    }
}
