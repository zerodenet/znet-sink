use znet_engine_client::configuration::{self, ApplyFailure, ConfigBackend as Backend};

use crate::errors::{AppError, AppResult};
use crate::kernel::{configuration::BoundControl, zero::queries};
use crate::models::core::CoreIpcOptions;
use serde_json::Value;

pub(crate) async fn apply(config: Value, options: CoreIpcOptions) -> AppResult<()> {
    apply_with_identity(config, options).await.map(|_| ())
}

pub(crate) async fn apply_with_identity(
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<queries::KernelRuntimeIdentity> {
    if !config.is_object() {
        return Err(AppError::invalid_argument("config must be a JSON object"));
    }
    let control = BoundControl::connect(options).await?;
    confirm(&Live { config, control }).await
}

struct Live {
    config: Value,
    control: BoundControl,
}

impl Backend for Live {
    type Error = AppError;
    async fn identity(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        self.control.identity().await
    }
    async fn submit(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        self.control.apply(self.config.clone()).await
    }
}

// Callers may persist only after this returns. A lost reply is not a rejection
// and must never trigger a second apply or an implicit process restart.
async fn confirm(
    backend: &impl Backend<Error = AppError>,
) -> AppResult<queries::KernelRuntimeIdentity> {
    configuration::confirm(backend)
        .await
        .map(|confirmed| confirmed.identity().clone())
        .map_err(|failure| match failure {
            ApplyFailure::Preflight(error) => error,
            ApplyFailure::Submission(mut error) => {
                // Once sent, a transport failure cannot prove non-application.
                if error.is_unavailable() || error.code == "invalid_response" {
                    error.code = "config_apply_uncertain";
                    error.message =
                        format!("配置提交结果不确定，请刷新运行状态：{}", error.message);
                }
                error
            }
            ApplyFailure::Confirmation(mut error) => {
                error.code = "config_apply_uncertain";
                error.message = format!("配置已提交但无法核对运行状态：{}", error.message);
                error
            }
            ApplyFailure::IdentityChanged => AppError::conflict(
                "config",
                "runtime",
                "配置应用期间内核或配置版本发生变化，未提交本地配置，请刷新后重试",
            ),
        })
}

#[cfg(test)]
#[path = "config_apply_tests.rs"]
mod tests;
