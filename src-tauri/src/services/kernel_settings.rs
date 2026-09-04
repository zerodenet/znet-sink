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
    if content.len() as u64 > MAX_IMPORT_BYTES {
        return Err(AppError::invalid_argument(
            "client kernel settings file must not exceed 2 MiB",
        ));
    }
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

    settings.tun.name = settings
        .tun
        .name
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    settings.tun.addr = settings.tun.addr.trim().to_owned();
    settings.tun.mask = settings.tun.mask.trim().to_owned();
    settings.tun.tag = settings.tun.tag.trim().to_owned();
    settings.tun.secondary_addr = settings
        .tun
        .secondary_addr
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    settings.tun.include_cidrs = super::app_config::normalize_tun_cidrs(
        std::mem::take(&mut settings.tun.include_cidrs),
        "tun.includeCidrs",
    )?;
    settings.tun.exclude_cidrs = super::app_config::normalize_tun_cidrs(
        std::mem::take(&mut settings.tun.exclude_cidrs),
        "tun.excludeCidrs",
    )?;

    let (tun_addr, tun_prefix) = validate_cidr(&settings.tun.addr, "tun.addr")?;
    let tun_mask = settings
        .tun
        .mask
        .parse::<IpAddr>()
        .map_err(|_| AppError::invalid_argument("tun.mask must be an IP address"))?;
    if tun_addr.is_ipv4() != tun_mask.is_ipv4() {
        return Err(AppError::invalid_argument(
            "tun.addr and tun.mask must use the same address family",
        ));
    }
    if !is_contiguous_mask(tun_mask) {
        return Err(AppError::invalid_argument(
            "tun.mask must be a contiguous network mask",
        ));
    }
    if network_mask_prefix(tun_mask) != Some(tun_prefix) {
        return Err(AppError::invalid_argument(
            "tun.addr prefix and tun.mask must describe the same network",
        ));
    }
    if settings.tun.tag.is_empty() {
        return Err(AppError::invalid_argument("tun.tag must not be empty"));
    }
    if settings.tun.mtu < 576 {
        return Err(AppError::invalid_argument("tun.mtu must be at least 576"));
    }
    let secondary_addr = if let Some(address) = settings.tun.secondary_addr.as_deref() {
        if !settings.tun.dual_stack {
            return Err(AppError::invalid_argument(
                "tun.secondaryAddr requires tun.dualStack=true",
            ));
        }
        let (address, _) = validate_cidr(address, "tun.secondaryAddr")?;
        if address.is_ipv4() == tun_addr.is_ipv4() {
            return Err(AppError::invalid_argument(
                "tun.addr and tun.secondaryAddr must use different address families",
            ));
        }
        Some(address)
    } else if settings.tun.dual_stack {
        Some(if tun_addr.is_ipv4() {
            "fd66::1".parse().expect("static IPv6 address")
        } else {
            "10.66.0.1".parse().expect("static IPv4 address")
        })
    } else {
        None
    };

    if let Some(dns) = settings.dns.config.as_ref() {
        dns.validate_client_shape()?;
    } else if settings.dns.enabled {
        return Err(AppError::invalid_argument(
            "dns.config is required when dns.enabled is true",
        ));
    }

    if settings.dns.enabled {
        let mut owned_addresses = vec![tun_addr];
        if let Some(address) = secondary_addr {
            owned_addresses.push(address);
        }
        owned_addresses.extend(owned_addresses.clone().into_iter().filter_map(next_ip));
        settings
            .dns
            .config
            .as_ref()
            .expect("enabled DNS config was checked")
            .validate_tun_owned_addresses(&owned_addresses)?;
    }
    let dns_hijack = settings.dns.enabled && settings.dns.dns_hijack;
    settings.dns.dns_hijack = dns_hijack;
    settings.tun.dns_hijack = dns_hijack;
    Ok(())
}

pub(crate) fn validate_cidr(value: &str, field: &str) -> AppResult<(IpAddr, u8)> {
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
    Ok((address, prefix))
}

pub(crate) fn is_contiguous_mask(mask: IpAddr) -> bool {
    let bits = match mask {
        IpAddr::V4(mask) => u32::from(mask) as u128,
        IpAddr::V6(mask) => u128::from(mask),
    };
    let relevant = if mask.is_ipv4() { bits << 96 } else { bits };
    let inverted = !relevant;
    inverted == 0 || (inverted & inverted.wrapping_add(1)) == 0
}

pub(crate) fn network_mask_prefix(mask: IpAddr) -> Option<u8> {
    is_contiguous_mask(mask).then(|| match mask {
        IpAddr::V4(mask) => u32::from(mask).count_ones() as u8,
        IpAddr::V6(mask) => u128::from(mask).count_ones() as u8,
    })
}

pub(crate) fn next_ip(address: IpAddr) -> Option<IpAddr> {
    match address {
        IpAddr::V4(address) => u32::from(address)
            .checked_add(1)
            .map(std::net::Ipv4Addr::from)
            .map(IpAddr::V4),
        IpAddr::V6(address) => u128::from(address)
            .checked_add(1)
            .map(std::net::Ipv6Addr::from)
            .map(IpAddr::V6),
    }
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
    fn sparse_legacy_app_config_keeps_historical_dns_and_tun_semantics() {
        let current = AppConfig::default();
        let imported = import_from_str(
            &current,
            &serde_json::json!({
                "schemaVersion": "gui.app.v1",
                "core": { "autoStart": false },
                "tun": { "mtu": 1280 }
            })
            .to_string(),
        )
        .unwrap();

        assert!(!imported.core.auto_start);
        assert_eq!(imported.tun.mtu, 1280);
        assert!(imported.tun.enabled.is_none());
        assert!(!imported.dns.enabled);
        assert!(imported.dns.config.is_none());
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

    #[test]
    fn imported_tun_values_are_normalized_before_persistence() {
        let current = AppConfig::default();
        let mut bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&current),
        };
        bundle.settings.tun.name = Some("  PortableTun  ".to_string());
        bundle.settings.tun.addr = " 10.66.0.1/30 ".to_string();
        bundle.settings.tun.mask = " 255.255.255.252 ".to_string();
        bundle.settings.tun.tag = " tun-in ".to_string();
        bundle.settings.tun.secondary_addr = Some(" fd66::1/64 ".to_string());
        bundle.settings.tun.include_cidrs = vec![" 0.0.0.0/0 ".to_string()];
        bundle.settings.tun.exclude_cidrs =
            vec![" 16.0.0.0/8 ".to_string(), "16.0.0.0/8".to_string()];

        let imported = import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).unwrap();

        assert_eq!(imported.tun.name.as_deref(), Some("PortableTun"));
        assert_eq!(imported.tun.addr, "10.66.0.1/30");
        assert_eq!(imported.tun.mask, "255.255.255.252");
        assert_eq!(imported.tun.tag, "tun-in");
        assert_eq!(imported.tun.secondary_addr.as_deref(), Some("fd66::1/64"));
        assert_eq!(imported.tun.include_cidrs, vec!["0.0.0.0/0"]);
        assert_eq!(imported.tun.exclude_cidrs, vec!["16.0.0.0/8"]);
    }

    #[test]
    fn invalid_imported_tun_route_cidr_never_produces_a_candidate() {
        let current = AppConfig::default();
        let mut bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&current),
        };
        bundle.settings.tun.exclude_cidrs = vec!["16.0.0.0/99".to_string()];

        assert!(import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).is_err());
    }

    #[test]
    fn invalid_tun_mask_or_secondary_address_never_produces_a_candidate() {
        let current = AppConfig::default();
        let mutations: [fn(&mut ClientKernelSettings); 3] = [
            |settings: &mut ClientKernelSettings| {
                settings.tun.mask = "255.0.255.0".to_string();
            },
            |settings: &mut ClientKernelSettings| {
                settings.tun.secondary_addr = Some("10.67.0.1/24".to_string());
            },
            |settings: &mut ClientKernelSettings| {
                settings.tun.dual_stack = false;
                settings.tun.secondary_addr = Some("fd66::1/64".to_string());
            },
        ];
        for mutate in mutations {
            let mut bundle = ClientKernelSettingsBundle {
                schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
                exported_at_unix_ms: 1,
                settings: ClientKernelSettings::from_app_config(&current),
            };
            mutate(&mut bundle.settings);
            assert!(import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).is_err());
        }
    }

    #[test]
    fn fake_ip_pool_overlapping_tun_is_rejected_before_persistence() {
        let current = AppConfig::default();
        let mut bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&current),
        };
        bundle.settings.dns.config = Some(
            serde_json::from_value(serde_json::json!({
                "servers": { "system": { "type": "system" } },
                "default_server": "system",
                "answer": { "type": "fake_ip", "cidr": "10.66.0.0/24" }
            }))
            .unwrap(),
        );

        assert!(import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).is_err());
    }

    #[test]
    fn disabled_but_invalid_dns_is_rejected_before_persistence() {
        let current = AppConfig::default();
        let mut bundle = ClientKernelSettingsBundle {
            schema_version: CLIENT_KERNEL_SETTINGS_SCHEMA.to_string(),
            exported_at_unix_ms: 1,
            settings: ClientKernelSettings::from_app_config(&current),
        };
        bundle.settings.dns.enabled = false;
        bundle.settings.dns.config = Some(
            serde_json::from_value(serde_json::json!({
                "servers": { "system": { "type": "system" } },
                "default_server": "system",
                "answer": { "type": "fake_ip", "cidr": "not-a-cidr" }
            }))
            .unwrap(),
        );

        assert!(import_from_str(&current, &serde_json::to_string(&bundle).unwrap()).is_err());
    }
}
