use serde::Serialize;
use serde_json::{json, Value};
use std::io;

use crate::client_core::ClientCoreError;
use crate::models::core::CoreEndpoint;

const CORE_INSUFFICIENT_OS_PRIVILEGE: &str = "insufficient_os_privilege";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl AppError {
    pub(crate) fn client_core(error: ClientCoreError) -> Self {
        let code = match error.code.as_str() {
            "active_profile_required" => "active_profile_required",
            "probe_targets_required" => "probe_targets_required",
            _ => "client_core_error",
        };
        Self {
            code,
            message: error.message,
            details: Some(json!({ "clientCoreCode": error.code })),
        }
    }

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_argument",
            message: message.into(),
            details: None,
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal",
            message: message.into(),
            details: None,
        }
    }

    pub(crate) fn not_found(resource: &'static str, id: impl Into<String>) -> Self {
        Self {
            code: "not_found",
            message: format!("{resource} not found"),
            details: Some(json!({ "resource": resource, "id": id.into() })),
        }
    }

    pub(crate) fn conflict(
        resource: &'static str,
        id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: "conflict",
            message: message.into(),
            details: Some(json!({ "resource": resource, "id": id.into() })),
        }
    }

    pub(crate) fn mode_restricted(
        feature: &'static str,
        required_mode: &'static str,
        current_mode: impl Into<String>,
    ) -> Self {
        Self {
            code: "mode_restricted",
            message: format!("{feature} is only available in {required_mode} mode"),
            details: Some(json!({
                "feature": feature,
                "requiredUiMode": required_mode,
                "uiMode": current_mode.into(),
            })),
        }
    }

    pub(crate) fn connection_closed(endpoint: &CoreEndpoint) -> Self {
        Self {
            code: "connection_closed",
            message: "core IPC connection closed before a response was received".to_string(),
            details: Some(json!({
                "transport": endpoint.transport,
                "endpoint": endpoint.path,
            })),
        }
    }

    pub(crate) fn core_response(response: Value) -> Self {
        let core_code = response
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str);
        let core_message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("core rejected the IPC request");

        let (code, message) = match core_code {
            Some(CORE_INSUFFICIENT_OS_PRIVILEGE) => (
                CORE_INSUFFICIENT_OS_PRIVILEGE,
                insufficient_os_privilege_message().to_string(),
            ),
            Some("not_found") => ("not_found", core_message.to_string()),
            Some("invalid_argument") => ("invalid_argument", core_message.to_string()),
            Some("permission_denied") => ("permission_denied", core_message.to_string()),
            Some("feature_disabled") => ("feature_disabled", core_message.to_string()),
            Some("conflict") => ("conflict", core_message.to_string()),
            Some("unsupported") => ("unsupported", core_message.to_string()),
            Some("internal") => ("internal", core_message.to_string()),
            _ => ("core_error", core_message.to_string()),
        };

        Self {
            code,
            message,
            // Preserve the complete Zero envelope, including its stable error
            // code and platform-specific cause, for logs and diagnostics.
            details: Some(response),
        }
    }

    pub(crate) fn from_io(context: &str, endpoint: &CoreEndpoint, error: io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::AddrNotAvailable
            | io::ErrorKind::AddrInUse => "core_unavailable",
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => "timeout",
            io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof => "connection_closed",
            _ => "io_error",
        };

        Self {
            code,
            message: format!("{context}: {error}"),
            details: Some(json!({
                "transport": endpoint.transport,
                "endpoint": endpoint.path,
                "ioKind": format!("{:?}", error.kind()),
            })),
        }
    }

    pub(crate) fn is_unavailable(&self) -> bool {
        matches!(
            self.code,
            "core_unavailable" | "timeout" | "connection_closed"
        )
    }
}

#[cfg(target_os = "windows")]
fn insufficient_os_privilege_message() -> &'static str {
    "TUN 需要管理员权限。请退出 ZNet-Sink，右键应用并选择“以管理员身份运行”，然后重新开启 TUN。"
}

#[cfg(target_os = "macos")]
fn insufficient_os_privilege_message() -> &'static str {
    "TUN 需要更高的系统网络权限。请以具备管理员权限的方式重新启动 ZNet-Sink，然后重新开启 TUN。"
}

#[cfg(target_os = "linux")]
fn insufficient_os_privilege_message() -> &'static str {
    "TUN 需要创建虚拟网卡和配置路由的系统权限。请以 root 运行，或为运行环境授予所需的网络 capabilities 后重试。"
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn insufficient_os_privilege_message() -> &'static str {
    "TUN 启动被操作系统拒绝：当前进程缺少所需的网络权限。请使用具备相应权限的方式重新启动后重试。"
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{AppError, CORE_INSUFFICIENT_OS_PRIVILEGE};

    #[test]
    fn promotes_core_os_privilege_error_and_preserves_diagnostics() {
        let error = AppError::core_response(json!({
            "ok": false,
            "error": {
                "code": "insufficient_os_privilege",
                "message": "TUN startup requires elevated host operating-system network privileges",
                "cause": "Windows TUN requires an elevated Administrator process"
            }
        }));

        assert_eq!(error.code, CORE_INSUFFICIENT_OS_PRIVILEGE);
        assert!(!error.message.is_empty());
        assert_ne!(
            error.message,
            "TUN startup requires elevated host operating-system network privileges"
        );
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.pointer("/error/code"))
                .and_then(|value| value.as_str()),
            Some(CORE_INSUFFICIENT_OS_PRIVILEGE)
        );
    }

    #[test]
    fn promotes_stable_core_error_codes() {
        let error = AppError::core_response(json!({
            "ok": false,
            "error": {
                "code": "conflict",
                "message": "runtime conflict"
            }
        }));

        assert_eq!(error.code, "conflict");
        assert_eq!(error.message, "runtime conflict");
    }

    #[test]
    fn keeps_unknown_core_errors_generic() {
        let error = AppError::core_response(json!({
            "ok": false,
            "error": { "code": "future_code", "message": "future error" }
        }));

        assert_eq!(error.code, "core_error");
        assert_eq!(error.message, "future error");
    }
}
