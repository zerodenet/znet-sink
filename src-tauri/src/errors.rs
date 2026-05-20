use serde::Serialize;
use serde_json::{json, Value};
use std::io;

use crate::models::core::CoreEndpoint;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl AppError {
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
        let message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("core rejected the IPC request")
            .to_string();

        Self {
            code: "core_error",
            message,
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

pub type AppResult<T> = Result<T, AppError>;
