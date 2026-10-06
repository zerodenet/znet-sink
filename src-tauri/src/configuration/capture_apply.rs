//! A profile change retains capture intent, not the old profile's TUN plan.
//! Callers hold proxy_config_operation and publish profiles only after success.
use crate::errors::{AppError, AppResult};
use crate::kernel::zero::{queries::KernelRuntimeIdentity, runtime};
use crate::models::{core::CoreIpcOptions, zero_runtime::GuiTunStatus};
use crate::state::app_state::AppState;
use serde_json::{json, Value};
use std::{future::Future, time::Duration};

mod parameters;
use parameters::{matches, matches_plan};

pub(crate) struct Snapshot {
    pub identity: KernelRuntimeIdentity,
    pub tun: GuiTunStatus,
}
trait Backend: Sync {
    fn snapshot(&self) -> impl Future<Output = AppResult<Snapshot>> + Send;
    fn apply(
        &self,
        config: &Value,
    ) -> impl Future<Output = AppResult<KernelRuntimeIdentity>> + Send;
    fn stop(&self) -> impl Future<Output = AppResult<()>> + Send;
    fn start(&self, params: &Value) -> impl Future<Output = AppResult<()>> + Send;
}
struct Live<'a> {
    state: &'a AppState,
    options: CoreIpcOptions,
}
pub(crate) struct Receipt {
    pub identity: KernelRuntimeIdentity,
    previous_config: Value,
    applied_config: Value,
    previous_params: Value,
    applied_params: Value,
    options: CoreIpcOptions,
}
impl Receipt {
    // Publication failures compensate the exact confirmed runtime transaction,
    // even when restoring profile storage has already changed local defaults.
    pub(crate) async fn restore(&self, state: &AppState) -> AppResult<()> {
        let backend = Live {
            state,
            options: self.options.clone(),
        };
        restore_receipt(
            &backend,
            &self.identity,
            &self.applied_config,
            &self.previous_config,
            &self.applied_params,
            &self.previous_params,
        )
        .await
    }
}
async fn restore_receipt(
    backend: &impl Backend,
    identity: &KernelRuntimeIdentity,
    applied: &Value,
    previous: &Value,
    applied_params: &Value,
    previous_params: &Value,
) -> AppResult<()> {
    owned_identity(identity, &backend.snapshot().await?.identity)?;
    apply(backend, applied, previous, applied_params, previous_params)
        .await
        .map(|_| ())
}
impl Backend for Live<'_> {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let identity =
            crate::kernel::zero::queries::runtime_identity(Some(self.options.clone())).await?;
        let tun = runtime::tun_status(Some(self.options.clone())).await?;
        owned_identity(
            &identity,
            &crate::kernel::zero::queries::runtime_identity(Some(self.options.clone())).await?,
        )?;
        Ok(Snapshot { identity, tun })
    }
    async fn apply(&self, config: &Value) -> AppResult<KernelRuntimeIdentity> {
        super::apply::apply_with_identity(
            self.state.capabilities(),
            config.clone(),
            self.options.clone(),
        )
        .await
    }
    async fn stop(&self) -> AppResult<()> {
        crate::services::native_operation::execute_async(
            self.state,
            "system.tun",
            "disable",
            Duration::from_secs(60),
            runtime::disable_tun(Some(self.options.clone())),
        )
        .await
        .map(|_| ())
    }
    async fn start(&self, params: &Value) -> AppResult<()> {
        crate::services::native_operation::execute_async(
            self.state,
            "system.tun",
            "enable",
            Duration::from_secs(60),
            runtime::enable_tun_params(params.clone(), Some(self.options.clone())),
        )
        .await
        .map(|_| ())
    }
}
pub(crate) async fn apply_profile(
    state: &AppState,
    source: &Value,
    profile: Option<&str>,
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<Receipt> {
    let app = crate::services::common::lock(state.app_config(), "app_config")?.clone();
    let (previous_app, previous_source) = super::preferences::current(state)?;
    let previous_params = super::preferences::tun_params_for(
        &previous_app,
        &previous_source,
        previous_app.tun.clone(),
    )?;
    let (next_app, next_source) = super::local_edits::resolve(&app, profile, source)?;
    let next_params =
        super::preferences::tun_params_for(&next_app, &next_source, next_app.tun.clone())?;
    let previous_id = crate::services::common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .map(|p| p.id.clone());
    let previous_config = crate::services::rule_overlay::compose_effective_config_for(
        state,
        &previous_source,
        previous_id.as_deref(),
    )?;
    let identity = apply(
        &Live {
            state,
            options: options.clone(),
        },
        &previous_config,
        &config,
        &previous_params,
        &next_params,
    )
    .await?;
    Ok(Receipt {
        identity,
        previous_config,
        applied_config: config,
        previous_params,
        applied_params: next_params,
        options,
    })
}
fn conflict() -> AppError {
    AppError::conflict(
        "tun",
        "configuration",
        "TUN 或内核运行状态已变化，请刷新后重试",
    )
}
fn owned_identity(
    expected: &KernelRuntimeIdentity,
    actual: &KernelRuntimeIdentity,
) -> AppResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(conflict())
    }
}
async fn observe(backend: &impl Backend, identity: &KernelRuntimeIdentity) -> AppResult<Snapshot> {
    let snapshot = backend.snapshot().await?;
    owned_identity(identity, &snapshot.identity)?;
    if snapshot.tun.managed_by_config {
        return Err(conflict());
    }
    Ok(snapshot)
}
fn uncertain(error: &AppError) -> bool {
    error.is_unavailable()
        || matches!(
            error.code,
            "conflict" | "invalid_response" | "config_apply_uncertain"
        )
        || error
            .details
            .as_ref()
            .is_some_and(|d| d["tunTransitionUncertain"] == true)
}
async fn transition(
    backend: &impl Backend,
    identity: &KernelRuntimeIdentity,
    from: Option<&Value>,
    to: Option<&Value>,
) -> AppResult<()> {
    let current = observe(backend, identity).await?;
    if !from.map_or(!current.tun.enabled, |p| {
        if to.is_none() {
            matches_plan(&current.tun, p)
        } else {
            matches(&current.tun, p)
        }
    }) {
        return Err(conflict());
    }
    let result = match to {
        Some(p) => backend.start(p).await,
        None => backend.stop().await,
    };
    if let Err(error) = result {
        if !error.is_unavailable() {
            return Err(error);
        }
    }
    // A lost acknowledgement does not license a second start/stop command.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match observe(backend, identity).await {
            Ok(s) if to.map_or(!s.tun.enabled, |p| matches(&s.tun, p)) => return Ok(()),
            Err(e) if e.code == "conflict" => return Err(e),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError {
                code: "timeout",
                message: "TUN 切换结果无法确认，请检查当前状态".into(),
                details: Some(json!({"tunTransitionUncertain":true})),
            });
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
async fn apply(
    backend: &impl Backend,
    old: &Value,
    next: &Value,
    previous: &Value,
    candidate: &Value,
) -> AppResult<KernelRuntimeIdentity> {
    let before = backend.snapshot().await?;
    if !before.tun.enabled || before.tun.managed_by_config {
        return backend.apply(next).await;
    }
    // Rule-only edits can use live reload. Rebuild capture when its plan,
    // DNS bootstrap, execution resources or outbound carriers change.
    let network_changed = [
        "/runtime/dns",
        "/runtime/network",
        "/endpoints",
        "/outbounds",
        "/inbounds",
    ]
    .iter()
    .any(|path| old.pointer(path) != next.pointer(path));
    if !network_changed && previous == candidate {
        return backend.apply(next).await;
    }
    if !matches(&before.tun, previous) {
        return Err(conflict());
    }
    let mut expected = before.identity;
    let mut applied = false;
    let result = async {
        transition(backend, &expected, Some(previous), None).await?;
        observe(backend, &expected).await?;
        let confirmed = backend.apply(next).await?;
        if confirmed.core_instance_id != expected.core_instance_id {
            return Err(conflict());
        }
        expected = confirmed;
        applied = true;
        observe(backend, &expected).await?;
        transition(backend, &expected, None, Some(candidate)).await?;
        Ok(expected.clone())
    }
    .await;
    let Err(mut error) = result else {
        return result;
    };
    let recovery = if uncertain(&error) {
        Err(AppError::internal("执行结果或归属不确定，未自动重发或回滚"))
    } else {
        async {
            let current = observe(backend, &expected).await?;
            if !applied && matches(&current.tun, previous) {
                return Ok(());
            }
            if current.tun.enabled {
                transition(
                    backend,
                    &expected,
                    Some(if applied { candidate } else { previous }),
                    None,
                )
                .await?;
            }
            if applied {
                let restored = backend.apply(old).await?;
                if restored.core_instance_id != expected.core_instance_id {
                    return Err(conflict());
                }
                expected = restored;
                observe(backend, &expected).await?;
            }
            transition(backend, &expected, None, Some(previous)).await
        }
        .await
    };
    error.message.push_str(if recovery.is_ok() {
        "；已恢复原配置和 TUN"
    } else {
        "；未能确认恢复原状态，请检查 TUN 和当前配置"
    });
    error.details = Some(
        json!({"cause":error.details,"captureRollback":{"succeeded":recovery.is_ok(),"error":recovery.err().map(|e|e.message)}}),
    );
    Err(error)
}
#[cfg(test)]
#[path = "capture_apply_tests.rs"]
mod tests;
