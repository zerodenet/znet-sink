//! Mobile host boundary. The UI consumes facts; native service/provider owns VPN.
//! Until the complete Proxy/external-TUN bridge exists, deny desktop fallbacks.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) mod application;

use serde::Serialize;
use znet_client_core::mobile::{reconcile, KernelState, VpnState};

use crate::errors::{AppError, AppResult};

pub const MOBILE_RUNTIME_REASON: &str = "移动 VPN 运行适配尚未完成：需要原生服务/扩展持有完整 Proxy、外部 TUN 接入、protect/bind 与网络变化回调，以及实例绑定的控制通道。当前不能开启或确认接管。";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobilePlatformSnapshot {
    pub platform: &'static str,
    pub mobile: bool,
    pub minimum_android_api: u32,
    pub runtime_owner: &'static str,
    pub bundled_kernel_available: bool,
    pub core_distribution: &'static str,
    pub adaptation_state: &'static str,
    pub vpn_supported: bool,
    pub reason: Option<&'static str>,
    pub kernel_state: KernelState,
    pub vpn_state: VpnState,
    pub observation_state: &'static str,
    pub core_instance_id: Option<String>,
    pub profile_id: Option<String>,
    pub config_revision: Option<String>,
}

pub fn snapshot() -> MobilePlatformSnapshot {
    let (platform, owner) = if cfg!(target_os = "android") {
        ("android", "android-vpn-service")
    } else if cfg!(target_os = "ios") {
        ("ios", "ios-packet-tunnel-provider")
    } else {
        ("desktop", "desktop-managed-process")
    };
    let mobile = platform != "desktop";
    // No native observations are available. Do not synthesize an instance or
    // claim a closed tunnel based on the lifetime of this WebView process.
    let reconciled = reconcile(None, None);
    MobilePlatformSnapshot {
        platform,
        mobile,
        minimum_android_api: 24,
        runtime_owner: owner,
        bundled_kernel_available: false,
        core_distribution: "bundled-with-application",
        adaptation_state: if mobile {
            "not-adapted"
        } else {
            "not-applicable"
        },
        vpn_supported: false,
        reason: mobile.then_some(MOBILE_RUNTIME_REASON),
        kernel_state: reconciled.kernel_state,
        vpn_state: reconciled.vpn_state,
        observation_state: "unsupported",
        core_instance_id: None,
        profile_id: None,
        config_revision: None,
    }
}

pub(crate) fn unavailable(operation: &str) -> AppError {
    AppError {
        code: "mobile_platform_unavailable",
        message: MOBILE_RUNTIME_REASON.into(),
        details: Some(serde_json::json!({
            "operation": operation,
            "platform": snapshot().platform,
            "adaptationState": "not-adapted",
            "coreDistribution": "bundled-with-application",
        })),
    }
}

/// Also gate internal callers, before spawning or acquiring OS resources.
pub(crate) fn require_desktop_runtime(operation: &str) -> AppResult<()> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Err(unavailable(operation));
    }
    Ok(())
}

#[tauri::command]
pub fn gui_mobile_platform_snapshot() -> MobilePlatformSnapshot {
    snapshot()
}

#[cfg(test)]
mod tests;
