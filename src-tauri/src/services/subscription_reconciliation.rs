//! Read-only inspection of an ordinary subscription's uncertain result.
//! Current storage has no durable candidate/apply receipt: a readable runtime
//! is evidence of reachability only, never permission to clear this barrier.
use crate::{
    errors::{AppError, AppResult},
    kernel::{configuration::BoundControl, zero::queries::KernelRuntimeIdentity},
    models::{core::CoreIpcOptions, subscription::SubscriptionProfile},
    services::common::{begin_in_flight, lock, normalize_required},
    state::app_state::AppState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use znet_client_core::capability::{Manager, Permission};

pub(crate) const UNKNOWN_APPLY_MESSAGE: &str =
    "订阅材料准备或运行提交结果未确认；请先核对已保存材料和运行状态，不要自动重放";
const UNKNOWN_REASON: &str =
    "没有持久候选材料及应用回执，当前运行身份可读也不能证明原订阅操作已生效；结果仍未确认。";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionReconciliation {
    pub status: &'static str,
    pub source: SourceFacts,
    pub material: MaterialFacts,
    pub evidence: EvidenceFacts,
    pub runtime: RuntimeFacts,
    pub can_resolve: bool,
    pub reason: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFacts {
    pub id: String,
    pub updated_at_unix_ms: String,
    pub last_sync_at_unix_ms: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialFacts {
    pub status: &'static str,
    pub profile_id: Option<String>,
    pub updated_at_unix_ms: Option<String>,
    pub active: Option<bool>,
    pub validation: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFacts {
    pub candidate: &'static str,
    pub receipt: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFacts {
    pub status: &'static str,
    pub core_instance_id: Option<String>,
    pub config_revision: Option<String>,
    pub reason_code: &'static str,
    pub reason: &'static str,
}
impl RuntimeFacts {
    fn unavailable() -> Self {
        Self::limited(
            "unavailable",
            "runtime_identity_unreadable",
            "内核运行身份无法完整读取；未核对原操作结果。",
        )
    }
    fn limited(status: &'static str, code: &'static str, reason: &'static str) -> Self {
        Self {
            status,
            core_instance_id: None,
            config_revision: None,
            reason_code: code,
            reason,
        }
    }
}
struct LocalSnapshot {
    source: SubscriptionProfile,
    source_fingerprint: String,
    material_fingerprint: Option<String>,
    material: MaterialFacts,
}
fn fingerprint(value: &impl Serialize) -> AppResult<String> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| AppError::internal("本地核对快照无法读取"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn local_snapshot(state: &AppState, id: &str, expected: u64) -> AppResult<LocalSnapshot> {
    let source = lock(state.subscriptions(), "subscription")?
        .iter()
        .find(|source| source.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("subscription", id))?;
    if source.managed_source.is_some() {
        return Err(AppError::invalid_argument("插件托管订阅须通过来源管理核对"));
    }
    if source.updated_at_unix_ms != expected {
        return Err(changed(id));
    }
    if source.last_error.as_deref() != Some(UNKNOWN_APPLY_MESSAGE) {
        return Err(AppError::invalid_argument(
            "此订阅没有可核对的宿主未确认状态，请刷新列表",
        ));
    }
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let profile = source
        .target_proxy_config_id
        .as_ref()
        .and_then(|target| profiles.iter().find(|profile| &profile.id == target));
    let mut material = MaterialFacts {
        status: if source.target_proxy_config_id.is_some() {
            "missing"
        } else {
            "not-linked"
        },
        profile_id: source.target_proxy_config_id.clone(),
        updated_at_unix_ms: None,
        active: None,
        validation: "not-available",
    };
    let material_fingerprint = match profile {
        None => None,
        Some(profile) => {
            material.updated_at_unix_ms = Some(profile.updated_at_unix_ms.to_string());
            material.active = Some(profile.active);
            if profile.managed_source.is_some() {
                material.status = "ownership-mismatch";
                material.validation = "not-inspected";
                // Do not read or clone a managed/protected body through this path.
                Some(fingerprint(&(
                    profile.id.clone(),
                    profile.updated_at_unix_ms,
                    "managed",
                ))?)
            } else {
                material.status = if profile.kernel == "zero"
                    && profile.content.as_ref().is_some_and(|body| {
                        zero_config::RuntimeConfig::parse(&body.to_string()).is_ok()
                    }) {
                    "saved-valid"
                } else {
                    "saved-invalid"
                };
                material.validation = "native-schema-only";
                Some(fingerprint(profile)?)
            }
        }
    };
    Ok(LocalSnapshot {
        source_fingerprint: fingerprint(&source)?,
        source,
        material_fingerprint,
        material,
    })
}
fn changed(id: &str) -> AppError {
    AppError::conflict(
        "subscription",
        id,
        "订阅来源或关联材料已变化，请刷新后重新核对",
    )
}
fn finish(
    state: &AppState,
    id: &str,
    expected: u64,
    before: LocalSnapshot,
    runtime: RuntimeFacts,
) -> AppResult<SubscriptionReconciliation> {
    let after = local_snapshot(state, id, expected)?;
    if before.source_fingerprint != after.source_fingerprint
        || before.material_fingerprint != after.material_fingerprint
    {
        return Err(changed(id));
    }
    Ok(SubscriptionReconciliation {
        status: "result-unknown",
        source: SourceFacts {
            id: before.source.id,
            updated_at_unix_ms: before.source.updated_at_unix_ms.to_string(),
            last_sync_at_unix_ms: before
                .source
                .last_sync_at_unix_ms
                .map(|value| value.to_string()),
        },
        material: before.material,
        evidence: EvidenceFacts {
            candidate: "not-recorded",
            receipt: "not-recorded",
        },
        runtime,
        can_resolve: false,
        reason: UNKNOWN_REASON,
    })
}

fn valid_identity(identity: &KernelRuntimeIdentity) -> bool {
    !identity.core_instance_id.trim().is_empty()
        && identity.core_instance_id.len() <= 512
        && !identity.core_instance_id.chars().any(char::is_control)
}
async fn runtime_identity(
    manager: &Manager,
    options: CoreIpcOptions,
    mobile: bool,
) -> RuntimeFacts {
    if mobile {
        return RuntimeFacts::limited(
            "unsupported",
            "mobile_runtime_bridge_not_adapted",
            "移动运行桥尚未接入，只能核对已保存的本地来源和材料，运行结果仍未确认。",
        );
    }
    let permission = Permission::new("runtime.inspect", "ordinary-subscription");
    let Ok(lease) = znet_client_capabilities::host::operation(
        manager,
        "builtin.subscription-reconciliation",
        permission.clone(),
        1,
        2048,
        Duration::from_secs(5),
    ) else {
        return RuntimeFacts::unavailable();
    };
    let result = lease
        .execute_async(&permission, async {
            let read = tokio::time::timeout(Duration::from_secs(5), async {
                let control = BoundControl::connect(options).await?;
                let before = control.identity().await?;
                let after = control.identity().await?;
                Ok::<_, AppError>((before, after))
            })
            .await
            .map_err(|_| znet_client_core::capability::Error::Deadline)?
            .map_err(|_| znet_client_core::capability::Error::Transport)?;
            Ok((read, 1024))
        })
        .await
        .and_then(|resource| resource.take(&lease));
    let Ok((before, after)) = result else {
        return RuntimeFacts::unavailable();
    };
    if !valid_identity(&before) || !valid_identity(&after) {
        return RuntimeFacts::unavailable();
    }
    if before != after {
        return RuntimeFacts::limited(
            "changed",
            "runtime_identity_changed",
            "核对期间内核实例或配置版本发生变化，运行结果仍未确认。",
        );
    }
    RuntimeFacts {
        status: "readable",
        core_instance_id: Some(after.core_instance_id),
        config_revision: Some(after.config_revision.to_string()),
        reason_code: "runtime_identity_only",
        reason: "当前运行身份稳定可读；没有原操作回执，不能据此确认原候选生效。",
    }
}

pub(crate) async fn inspect(
    state: &AppState,
    id: String,
    expected_updated_at_unix_ms: u64,
) -> AppResult<SubscriptionReconciliation> {
    let id = normalize_required(id, "id")?;
    let _claim = begin_in_flight(state.subscription_syncs(), "subscription", &id)?;
    let _profile = state.proxy_config_operation().lock().await;
    let before = local_snapshot(state, &id, expected_updated_at_unix_ms)?;
    let core = lock(state.app_config(), "app_config")?.core.clone();
    let core_fingerprint = fingerprint(&core)?;
    let runtime = if core.kernel != "zero" {
        RuntimeFacts::limited(
            "unsupported",
            "kernel_identity_not_supported",
            "当前内核没有受支持的运行身份读取接口，结果仍未确认。",
        )
    } else {
        runtime_identity(
            state.capabilities(),
            super::core_config::ipc_options_from_app_config(&core),
            crate::mobile_host::snapshot().mobile,
        )
        .await
    };
    if fingerprint(&lock(state.app_config(), "app_config")?.core)? != core_fingerprint {
        return Err(changed(&id));
    }
    finish(state, &id, expected_updated_at_unix_ms, before, runtime)
}

#[cfg(test)]
#[path = "subscription_reconciliation_tests.rs"]
mod tests;
