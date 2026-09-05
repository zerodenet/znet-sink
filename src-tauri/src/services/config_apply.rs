use std::future::Future;

use crate::errors::{AppError, AppResult};
use crate::kernel::{
    adapter::KernelAdapter,
    zero::{queries, ZeroAdapter},
};
use crate::models::core::CoreIpcOptions;
use serde_json::Value;

pub(crate) async fn apply(config: Value, options: CoreIpcOptions) -> AppResult<()> {
    confirm(&Live { config, options }).await
}

trait Backend {
    fn identity(&self) -> impl Future<Output = AppResult<queries::KernelRuntimeIdentity>> + Send;
    fn submit(&self) -> impl Future<Output = AppResult<queries::KernelRuntimeIdentity>> + Send;
}

struct Live {
    config: Value,
    options: CoreIpcOptions,
}

impl Backend for Live {
    async fn identity(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        queries::runtime_identity(Some(self.options.clone())).await
    }
    async fn submit(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        let result = ZeroAdapter::new()
            .apply_config(self.config.clone(), self.options.clone())
            .await?;
        queries::config_apply_identity(&result).map_err(|mut error| {
            error.code = "config_apply_uncertain";
            error.message = format!("配置应用结果无法确认，请刷新运行状态：{}", error.message);
            error
        })
    }
}

// Callers may persist only after this returns. A lost reply is not a rejection
// and must never trigger a second apply or an implicit process restart.
async fn confirm(backend: &impl Backend) -> AppResult<()> {
    let before = backend.identity().await?;
    let applied = backend.submit().await?;
    let current = backend.identity().await.map_err(|mut error| {
        error.code = "config_apply_uncertain";
        error.message = format!("配置已提交但无法核对运行状态：{}", error.message);
        error
    })?;
    if before.core_instance_id != applied.core_instance_id || current != applied {
        return Err(AppError::conflict(
            "config",
            "runtime",
            "配置应用期间内核或配置版本发生变化，未提交本地配置，请刷新后重试",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_apply_tests.rs"]
mod tests;
