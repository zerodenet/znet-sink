//! Mobile host boundary. The UI consumes facts; native service/provider owns VPN.
//! Until the complete Proxy/external-TUN bridge exists, deny desktop fallbacks.
#[cfg(target_os = "android")]
pub(crate) mod android_jobs;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) mod application;

use serde::Serialize;
use znet_client_core::mobile::{reconcile, KernelState, VpnState};

use crate::errors::{AppError, AppResult};

pub const MOBILE_RUNTIME_REASON: &str = "移动 VPN 运行适配尚未完成：需要原生服务/扩展持有完整 Proxy、外部 TUN 接入、protect/bind 与网络变化回调，以及实例绑定的控制通道。当前不能开启或确认接管。";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedCapability {
    pub capability_id: &'static str,
    pub status: &'static str,
    pub limitation_kind: &'static str,
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub dependency_ids: &'static [&'static str],
}

fn unsupported_capabilities(mobile: bool) -> Vec<UnsupportedCapability> {
    if !mobile {
        return Vec::new();
    }
    vec![
        UnsupportedCapability {
            capability_id: "vpn.forwarding",
            status: "unsupported",
            limitation_kind: "not-adapted",
            reason_code: "mobile_tunnel_and_egress_not_adapted",
            reason: "VPN 暂不支持：系统隧道接入及隧道外网络保护尚未适配，不能确认接管。",
            dependency_ids: &["CORE-01", "CORE-02", "CORE-05", "CLIENT-01"],
        },
        UnsupportedCapability {
            capability_id: "kernel.mobile-runtime",
            status: "unsupported",
            limitation_kind: "not-adapted",
            reason_code: "native_runtime_owner_not_registered",
            reason:
                "移动内核运行暂不支持：原生服务及控制通道尚未接入，完整内核的移动目标编译仍受阻。",
            dependency_ids: &["CLIENT-01", "CORE-05"],
        },
        UnsupportedCapability {
            capability_id: "kernel.bundled-artifact",
            status: "unsupported",
            limitation_kind: "not-adapted",
            reason_code: "runnable_mobile_kernel_not_bundled",
            reason: "默认内核将随应用分发；当前尚无可运行的移动内核产物。",
            dependency_ids: &["CLIENT-01", "CORE-05"],
        },
    ]
}

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
    pub unsupported_capabilities: Vec<UnsupportedCapability>,
}

pub fn snapshot() -> MobilePlatformSnapshot {
    let platform = if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "desktop"
    };
    snapshot_for(platform)
}

fn snapshot_for(platform: &'static str) -> MobilePlatformSnapshot {
    let owner = match platform {
        "android" => "android-vpn-service",
        "ios" => "ios-packet-tunnel-provider",
        _ => "desktop-managed-process",
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
        unsupported_capabilities: unsupported_capabilities(mobile),
    }
}

pub(crate) fn unavailable(operation: &str) -> AppError {
    AppError {
        code: "unsupported",
        message: MOBILE_RUNTIME_REASON.into(),
        details: Some(serde_json::json!({
            "operation": operation,
            "platform": snapshot().platform,
            "adaptationState": "not-adapted",
            "status": "unsupported",
            "reasonCode": "native_runtime_owner_not_registered",
            "dependencyIds": ["CLIENT-01", "CORE-01", "CORE-02", "CORE-05"],
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

#[tauri::command]
pub fn gui_mobile_subscription_schedule_snapshot(
    state: tauri::State<'_, crate::state::app_state::AppState>,
    window: tauri::Window,
) -> AppResult<crate::services::mobile_subscription_scheduler::MobileScheduleSnapshot> {
    crate::services::native_documents::require_host(window.label())?;
    if !snapshot().mobile {
        return Err(AppError {
            code: "unsupported",
            message: "当前宿主没有原生移动生命周期；移动后台执行仍未适配。".into(),
            details: Some(
                serde_json::json!({"status":"unsupported","reasonCode":"native_foreground_lifecycle_not_available","dependencyIds":["CLIENT-02"]}),
            ),
        });
    }
    state.mobile_subscription_scheduler().snapshot()
}

#[cfg(test)]
mod tests;
