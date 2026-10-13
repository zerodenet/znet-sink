//! Mobile/other hosts do not implement desktop OS proxy mutation or inspection.
//! Missing support must not produce a disabled/healthy proxy snapshot.
use super::{AppError, AppResult, ProxyBackup, SystemProxyStatus};

pub(super) fn unavailable() -> AppError {
    AppError {
        code: "platform_unsupported",
        message: "当前平台不支持桌面系统代理接口；移动接管必须通过原生 VPN 服务/扩展协调，不能以桌面代理代替。".into(),
        details: Some(serde_json::json!({ "capability": "desktop.system-proxy" })),
    }
}

pub(super) fn set_proxy_platform(
    _host: &str,
    _port: u16,
    _enable: bool,
    _socks_enabled: bool,
    _bypass: &[String],
) -> AppResult<()> {
    Err(unavailable())
}

pub(super) fn status_platform() -> AppResult<SystemProxyStatus> {
    Err(unavailable())
}

pub(super) fn capture_backup_platform() -> AppResult<ProxyBackup> {
    Err(unavailable())
}

pub(super) fn restore_platform(_backup: &ProxyBackup) -> AppResult<()> {
    Err(unavailable())
}

pub(super) fn local_bypass_configured_platform() -> Option<bool> {
    None
}
