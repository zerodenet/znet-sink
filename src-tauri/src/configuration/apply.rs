use znet_engine_client::configuration::{self, ApplyFailure, ConfigBackend as Backend};

use crate::errors::{AppError, AppResult};
use crate::kernel::{configuration::BoundControl, zero::queries};
use crate::models::core::CoreIpcOptions;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Lease, Manager, Permission};

pub(crate) async fn apply(
    manager: &Manager,
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<()> {
    apply_with_identity(manager, config, options)
        .await
        .map(|_| ())
}

pub(crate) async fn apply_with_identity(
    manager: &Manager,
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<queries::KernelRuntimeIdentity> {
    apply_with_control_source(manager, config, ControlSource::Discover(options)).await
}

/// A workspace plan keeps the original connection and identity. This guard
/// does not add a remote atomic precondition to config.apply_runtime.
pub(crate) async fn apply_with_bound_identity(
    manager: &Manager,
    config: Value,
    control: BoundControl,
    expected: &queries::KernelRuntimeIdentity,
) -> AppResult<queries::KernelRuntimeIdentity> {
    apply_with_control_source(
        manager,
        config,
        ControlSource::Fixed(control, expected.clone()),
    )
    .await
}

enum ControlSource {
    Discover(CoreIpcOptions),
    Fixed(BoundControl, queries::KernelRuntimeIdentity),
}

async fn apply_with_control_source(
    manager: &Manager,
    config: Value,
    source: ControlSource,
) -> AppResult<queries::KernelRuntimeIdentity> {
    let permission = Permission::new("configuration.apply", "active-runtime");
    let grants = BTreeSet::from([permission]);
    let failure = |error| AppError::invalid_argument(format!("配置能力不可用：{error}"));
    let policy = manager
        .admit(
            "builtin.configuration".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .map_err(failure)?;
    policy
        .authorize(grants, Duration::from_secs(60))
        .map_err(failure)?;
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 4096,
                timeout: Duration::from_secs(60),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(failure)?;
    match source {
        ControlSource::Discover(options) => apply_authorized(&lease, config, options).await,
        fixed => {
            apply_authorized_source(
                &lease,
                Permission::new("configuration.apply", "active-runtime"),
                config,
                fixed,
            )
            .await
        }
    }
}

pub(crate) async fn apply_authorized(
    lease: &Lease,
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<queries::KernelRuntimeIdentity> {
    let permission = Permission::new("configuration.apply", "active-runtime");
    apply_authorized_for(lease, permission, config, options).await
}

pub(crate) async fn apply_authorized_for(
    lease: &Lease,
    permission: Permission,
    config: Value,
    options: CoreIpcOptions,
) -> AppResult<queries::KernelRuntimeIdentity> {
    apply_authorized_source(lease, permission, config, ControlSource::Discover(options)).await
}

async fn apply_authorized_source(
    lease: &Lease,
    permission: Permission,
    config: Value,
    source: ControlSource,
) -> AppResult<queries::KernelRuntimeIdentity> {
    lease
        .check(Some(&permission))
        .map_err(|reason| AppError::invalid_argument(format!("配置操作未获授权：{reason}")))?;
    let mut backend_error = None;
    let result = lease
        .execute_async(&permission, async {
            let result = async {
                if !config.is_object() {
                    return Err(AppError::invalid_argument("config must be a JSON object"));
                }
                let (control, expected) = match source {
                    ControlSource::Discover(options) => {
                        (BoundControl::connect(options).await?, None)
                    }
                    ControlSource::Fixed(control, expected) => (control, Some(expected)),
                };
                let target = control.identity().await?;
                ensure_expected_identity(expected.as_ref(), &target)?;
                let _claim = lease
                    .claim_resource(format!("configuration:{}", target.core_instance_id))
                    .map_err(|reason| {
                        AppError::conflict(
                            "configuration",
                            "runtime",
                            format!("已有配置操作正在执行：{reason}"),
                        )
                    })?;
                confirm(&Live {
                    config,
                    control,
                    lease,
                    permission: permission.clone(),
                    expected,
                })
                .await
            }
            .await;
            result.map(|identity| (identity, 256)).map_err(|error| {
                backend_error = Some(error);
                znet_client_core::capability::Error::Transport
            })
        })
        .await
        .and_then(|resource| resource.take(lease));
    result.map_err(|reason| {
        backend_error.unwrap_or_else(|| AppError {
            code: "config_apply_uncertain",
            message: format!("配置操作授权已失效或结果无法交付，请核对实际运行状态：{reason}"),
            details: None,
        })
    })
}

struct Live<'a> {
    lease: &'a Lease,
    config: Value,
    control: BoundControl,
    permission: Permission,
    expected: Option<queries::KernelRuntimeIdentity>,
}

impl Backend for Live<'_> {
    type Error = AppError;
    async fn identity(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        self.control.identity().await
    }
    async fn submit(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        self.lease
            .check(Some(&self.permission))
            .map_err(|error| AppError::invalid_argument(format!("配置提交授权失效：{error}")))?;
        // Do not adopt a new instance/revision as a fresh baseline after the
        // caller's plan. The check and command still are separate wire calls.
        if self.expected.is_some() {
            ensure_expected_identity(self.expected.as_ref(), &self.control.identity().await?)?;
        }
        self.control.apply(self.config.clone()).await
    }
}

fn ensure_expected_identity(
    expected: Option<&queries::KernelRuntimeIdentity>,
    actual: &queries::KernelRuntimeIdentity,
) -> AppResult<()> {
    if expected.is_some_and(|expected| expected != actual) {
        return Err(AppError::conflict(
            "config",
            "runtime",
            "原预检运行身份已变化，未改用新实例或版本提交配置",
        ));
    }
    Ok(())
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
