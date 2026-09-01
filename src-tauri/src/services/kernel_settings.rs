use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::models::app_config::{
    AppConfig, ClientKernelSettings, ClientKernelSettingsBundle, CLIENT_KERNEL_SETTINGS_SCHEMA,
};

const MAX_IMPORT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelSettingsExportResult {
    pub path: String,
    pub schema_version: String,
}

pub fn export_to_path(
    config: &AppConfig,
    path: impl AsRef<Path>,
) -> AppResult<KernelSettingsExportResult> {
    let path = normalized_path(path)?;
    let bundle = ClientKernelSettingsBundle {
        schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
        exported_at_unix_ms: crate::services::common::now_unix_ms(),
        settings: ClientKernelSettings::from_app_config(config),
    };
    let content = serde_json::to_string_pretty(&bundle).map_err(|error| {
        AppError::internal(format!(
            "failed to serialize client kernel settings: {error}"
        ))
    })?;
    fs::write(&path, format!("{content}\n")).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to export client kernel settings: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    Ok(KernelSettingsExportResult {
        path: path.display().to_string(),
        schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
    })
}

pub fn import_from_path(current: &AppConfig, path: impl AsRef<Path>) -> AppResult<AppConfig> {
    let path = normalized_path(path)?;
    let metadata = fs::metadata(&path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to inspect client kernel settings: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(AppError::invalid_argument(
            "client kernel settings file must not exceed 2 MiB",
        ));
    }
    let content = fs::read_to_string(&path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read client kernel settings: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    import_from_str(current, &content)
}

pub(crate) fn import_from_str(current: &AppConfig, content: &str) -> AppResult<AppConfig> {
    let value: Value = serde_json::from_str(content).map_err(|error| {
        AppError::invalid_argument(format!("failed to parse client kernel settings: {error}"))
    })?;
    let schema = value
        .get("schemaVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::invalid_argument("client kernel settings requires schemaVersion")
        })?;
    let mut settings = match schema {
        CLIENT_KERNEL_SETTINGS_SCHEMA => {
            serde_json::from_value::<ClientKernelSettingsBundle>(value)
                .map_err(|error| {
                    AppError::invalid_argument(format!(
                        "invalid client kernel settings bundle: {error}"
                    ))
                })?
                .settings
        }
        "gui.app.v1" => {
            let legacy = serde_json::from_value::<AppConfig>(value).map_err(|error| {
                AppError::invalid_argument(format!("invalid legacy app configuration: {error}"))
            })?;
            ClientKernelSettings::from_app_config(&legacy)
        }
        unsupported => {
            return Err(AppError::invalid_argument(format!(
                "unsupported client kernel settings schema `{unsupported}`"
            )));
        }
    };
    normalize_and_validate(&mut settings)?;
    let mut next = current.clone();
    settings.apply_to(&mut next);
    Ok(next)
}

fn normalize_and_validate(settings: &mut ClientKernelSettings) -> AppResult<()> {
    settings.core.network_probe_urls =
        super::app_config::normalize_network_probe_urls(settings.core.network_probe_urls.clone())?;

    let tun_addr = validate_cidr(&settings.tun.addr, "tun.addr")?;
    let tun_mask = settings
        .tun
        .mask
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| AppError::invalid_argument("tun.mask must be an IP address"))?;
    if tun_addr.is_ipv4() != tun_mask.is_ipv4() {
        return Err(AppError::invalid_argument(
            "tun.addr and tun.mask must use the same address family",
        ));
    }
    if settings.tun.tag.trim().is_empty() {
        return Err(AppError::invalid_argument("tun.tag must not be empty"));
    }
    if settings.tun.mtu < 576 {
        return Err(AppError::invalid_argument("tun.mtu must be at least 576"));
    }
    if let Some(address) = settings.tun.secondary_addr.as_deref() {
        validate_cidr(address, "tun.secondaryAddr")?;
    }

    if settings.dns.enabled {
        settings
            .dns
            .config
            .as_ref()
            .ok_or_else(|| {
                AppError::invalid_argument("dns.config is required when dns.enabled is true")
            })?
            .validate_client_shape()?;
    }
    let dns_hijack = settings.dns.enabled && settings.dns.dns_hijack;
    settings.dns.dns_hijack = dns_hijack;
    settings.tun.dns_hijack = dns_hijack;
    Ok(())
}

fn validate_cidr(value: &str, field: &str) -> AppResult<IpAddr> {
    let (address, prefix) = value
        .trim()
        .split_once('/')
        .ok_or_else(|| AppError::invalid_argument(format!("{field} must use CIDR notation")))?;
    let address = address.parse::<IpAddr>().map_err(|_| {
        AppError::invalid_argument(format!("{field} contains an invalid IP address"))
    })?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| AppError::invalid_argument(format!("{field} contains an invalid prefix")))?;
    let maximum = if address.is_ipv4() { 32 } else { 128 };
    if prefix > maximum {
        return Err(AppError::invalid_argument(format!(
            "{field} prefix must be between 0 and {maximum}"
        )));
    }
    Ok(address)
}

fn normalized_path(path: impl AsRef<Path>) -> AppResult<PathBuf> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(AppError::invalid_argument(
            "client kernel settings path must not be empty",
        ));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_round_trip_preserves_portable_settings_only() {
        let mut source = AppConfig::default();
        source.core.executable_path = Some("C:/machine-a/zero.exe".to_string());
        source.core.socket = Some("machine-a-pipe".to_string());
        source.tun.mtu = 1400;
        source.ui.theme = "dark".to_string();
        let bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&source),
        };
        let content = serde_json::to_string(&bundle).unwrap();

        assert!(!content.contains("machine-a"));
        let mut current = AppConfig::default();
        current.core.executable_path = Some("C:/machine-b/zero.exe".to_string());
        current.ui.theme = "light".to_string();
        let imported = import_from_str(&current, &content).unwrap();

        assert_eq!(imported.tun.mtu, 1400);
        assert_eq!(
            imported.core.executable_path.as_deref(),
            Some("C:/machine-b/zero.exe")
        );
        assert_eq!(imported.ui.theme, "light");
    }

    #[test]
    fn legacy_app_config_is_projected_into_portable_settings() {
        let mut legacy = AppConfig::default();
        legacy.schema_version = "gui.app.v1".to_string();
        legacy.tun.mtu = 1280;
        legacy.logs.level = "trace".to_string();
        let imported = import_from_str(
            &AppConfig::default(),
            &serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        assert_eq!(imported.tun.mtu, 1280);
        assert_eq!(imported.logs.level, "info");
    }

    #[test]
    fn invalid_or_future_bundle_never_produces_a_candidate() {
        let current = AppConfig::default();
        assert!(import_from_str(&current, "{not-json").is_err());
        assert!(import_from_str(
            &current,
            r#"{"schemaVersion":"znet.client-kernel-settings.v2","settings":{}}"#,
        )
        .is_err());
    }

    #[test]
    fn invalid_tun_cidr_never_produces_a_candidate() {
        let current = AppConfig::default();
        let mut bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&current),
        };
        bundle.settings.tun.addr = "10.66.0.1/99".to_string();

        assert!(import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).is_err());
    }
}
