use crate::errors::{AppError, AppResult};
use crate::kernel::zero;
use crate::models::app_config::{AppConfig, AppConfigPatch, AppTunConfig, AppTunConfigPatch};
use crate::models::core::CoreIpcOptions;
use crate::models::core_process::CoreProcessState;
use crate::services::{app_config, common, core_config, core_process};
use crate::state::app_state::AppState;

#[cfg(test)]
mod tests;
pub(crate) mod transaction;
mod validation;

use transaction::{Backend, Snapshot};

// The caller holds the same operation lock as profile changes and TUN toggles.
pub(super) async fn apply(state: &AppState, patch: AppTunConfigPatch) -> AppResult<AppConfig> {
    if patch.enabled.is_some() {
        return Err(AppError::invalid_argument(
            "use the TUN toggle to change its enabled state",
        ));
    }
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    let mut candidate = app_config::prepare_update(
        &previous,
        AppConfigPatch {
            tun: Some(patch),
            ..Default::default()
        },
    )?;
    validation::validate(&mut candidate)?;
    if crate::configuration::preferences::owns_tun(state)? {
        app_config::replace(state, candidate.clone())?;
        return Ok(candidate);
    }
    let running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    let backend = LiveBackend {
        state,
        options: core_config::ipc_options_from_app_config(&previous.core),
    };
    transaction::apply(&backend, &previous, &candidate, running).await?;
    Ok(candidate)
}

struct LiveBackend<'a> {
    state: &'a AppState,
    options: CoreIpcOptions,
}

impl Backend for LiveBackend<'_> {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let identity = zero::queries::runtime_identity(Some(self.options.clone())).await?;
        let status = zero::runtime::tun_status(Some(self.options.clone())).await?;
        let current = zero::queries::runtime_identity(Some(self.options.clone())).await?;
        if identity != current {
            return Err(AppError::conflict(
                "tun",
                "runtime",
                "core changed while reading TUN state",
            ));
        }
        Ok(Snapshot { identity, status })
    }

    async fn stop(&self) -> AppResult<()> {
        zero::runtime::disable_tun(Some(self.options.clone()))
            .await
            .map(|_| ())
    }

    async fn start(&self, tun: &AppTunConfig) -> AppResult<()> {
        zero::runtime::enable_tun(tun.clone(), Some(self.options.clone()))
            .await
            .map(|_| ())
    }

    fn persist(&self, config: &AppConfig) -> AppResult<()> {
        app_config::replace(self.state, config.clone())
    }
}
