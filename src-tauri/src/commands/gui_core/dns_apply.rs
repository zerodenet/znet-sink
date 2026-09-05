use super::{default_opts, GuiDnsSettingsInput};
use crate::commands::app_config::tun_settings::transaction::{Backend, Snapshot};
use crate::errors::{AppError, AppResult};
use crate::kernel::{
    adapter::KernelAdapter,
    zero::{self, ZeroAdapter},
};
use crate::models::{
    app_config::{AppConfig, AppDnsConfig, AppTunConfig},
    core_process::CoreProcessState,
};
use crate::services::{app_config, common, core_config, core_process, rule_overlay};
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::State;

mod transaction;

pub(super) async fn apply(
    state: State<'_, AppState>,
    input: GuiDnsSettingsInput,
) -> AppResult<Value> {
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    let mut candidate = previous.clone();
    candidate.dns = AppDnsConfig {
        enabled: input.enabled,
        config: input.config,
        dns_hijack: input.enabled && input.dns_hijack,
    };
    candidate.tun.dns_hijack = candidate.dns.dns_hijack;
    let content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone());
    let configs = content
        .as_ref()
        .map(|content| {
            Ok::<_, AppError>((
                rule_overlay::compose_effective_config_with_dns(
                    state.inner(),
                    content,
                    &previous.dns,
                )?,
                rule_overlay::compose_effective_config_with_dns(
                    state.inner(),
                    content,
                    &candidate.dns,
                )?,
            ))
        })
        .transpose()?;
    let running = core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let backend = Live {
        state: state.clone(),
        export: content.is_some(),
    };
    if let Some((old, next)) = configs.filter(|_| running) {
        transaction::apply(&backend, &previous, &candidate, &old, &next).await
    } else {
        if let Err(error) = backend.persist(&candidate) {
            let recovery = backend.persist(&previous);
            return Err(super::dns_transaction::with_rollback_status(
                error,
                true,
                recovery.is_ok(),
                "storage_restore",
                recovery.err(),
            ));
        }
        Ok(serde_json::json!({"ok":true,"applied":false,"reason":"no_active_running_profile"}))
    }
}

struct Live<'a> {
    state: State<'a, AppState>,
    export: bool,
}

impl Backend for Live<'_> {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let identity =
            zero::queries::runtime_identity(Some(default_opts(self.state.inner()))).await?;
        let status = zero::runtime::tun_status(Some(default_opts(self.state.inner()))).await?;
        let current =
            zero::queries::runtime_identity(Some(default_opts(self.state.inner()))).await?;
        if identity != current {
            return Err(AppError::conflict(
                "dns",
                "runtime",
                "core changed while reading DNS/TUN state",
            ));
        }
        Ok(Snapshot { identity, status })
    }
    async fn stop(&self) -> AppResult<()> {
        zero::runtime::disable_tun(Some(default_opts(self.state.inner())))
            .await
            .map(|_| ())
    }
    async fn start(&self, tun: &AppTunConfig) -> AppResult<()> {
        zero::runtime::enable_tun(tun.clone(), Some(default_opts(self.state.inner())))
            .await
            .map(|_| ())
    }
    fn persist(&self, config: &AppConfig) -> AppResult<()> {
        app_config::replace(self.state.inner(), config.clone())?;
        if self.export {
            core_config::export_active(self.state.clone())?;
        }
        Ok(())
    }
}

impl transaction::Backend for Live<'_> {
    async fn apply_dns(
        &self,
        config: &Value,
    ) -> AppResult<(Value, zero::queries::KernelRuntimeIdentity)> {
        let value = ZeroAdapter::new()
            .apply_config(config.clone(), default_opts(self.state.inner()))
            .await?;
        let identity = zero::queries::config_apply_identity(&value).map_err(|mut error| {
            error.code = "dns_apply_uncertain";
            error
        })?;
        Ok((value, identity))
    }
}
