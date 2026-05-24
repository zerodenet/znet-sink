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
    pub download_url: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfigExportResult {
    pub proxy_config_id: String,
    pub path: String,
    pub app_config: CoreConfigSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreDownloadResult {
    pub success: bool,
    pub executable_path: String,
    pub version: Option<String>,
    pub message: String,
}

impl CoreConfigSnapshot {
    pub fn validate_launchable(&self) -> Result<(), String> {
        let executable_path = self
            .executable_path
            .as_deref()
            .ok_or_else(|| "core executable path is not configured".to_string())?;
        if !self.executable_exists {
            return Err(format!("core executable does not exist: {executable_path}"));
        }
        if let Some(false) = self.working_dir_exists {
            return Err(format!(
                "core working directory does not exist: {}",
                self.working_dir.as_deref().unwrap_or_default()
            ));
        }
        let config_path = self
            .config_path
            .as_deref()
            .ok_or_else(|| "core config file is not configured".to_string())?;
        if !self.config_exists.unwrap_or(false) {
            return Err(format!("core config file does not exist: {config_path}"));
        }

        Ok(())
    }
}
