//! Bind kernel transactions to client-owned configuration preferences.
use super::{Action, Catalog, ControlInput, Directions, Persistence};
use crate::configuration::endpoints;
use crate::errors::{AppError, AppResult};
use crate::models::{app_config::AppConfig, core::CoreIpcOptions};
use crate::services::common::lock;
use crate::state::app_state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManagedAction {
    SetState { enabled: bool },
    SetDirections { directions: Directions },
    Restart {},
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedInput {
    pub profile_id: String,
    pub config_revision: u64,
    pub endpoint_id: String,
    pub core_instance_id: String,
    pub expected_intent_revision: u64,
    pub action: ManagedAction,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCatalog {
    #[serde(flatten)]
    pub catalog: Catalog,
    pub profile_id: Option<String>,
    pub editable_endpoint_ids: Vec<String>,
    pub local_override_ids: Vec<String>,
}
fn active(state: &AppState) -> AppResult<Option<(String, Value)>> {
    Ok(lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| {
            profile
                .content
                .clone()
                .map(|content| (profile.id.clone(), content))
        }))
}
pub async fn catalog(state: &AppState, options: CoreIpcOptions) -> AppResult<ManagedCatalog> {
    let catalog = super::catalog(options).await?;
    let source = active(state)?;
    let app = lock(state.app_config(), "app_config")?;
    let mut editable_endpoint_ids = Vec::new();
    let mut local_override_ids = Vec::new();
    if let Some((id, source)) = &source {
        for row in &catalog.endpoints {
            if endpoints::owns(source, row) {
                let endpoint_id = row["endpoint_id"].as_str().unwrap().to_owned();
                if endpoints::has_override(&app, id, row) {
                    local_override_ids.push(endpoint_id.clone());
                }
                editable_endpoint_ids.push(endpoint_id);
            }
        }
    }
    Ok(ManagedCatalog {
        catalog,
        profile_id: source.map(|(id, _)| id),
        editable_endpoint_ids,
        local_override_ids,
    })
}
fn prepare(
    input: ManagedInput,
    profile: &str,
    source: &Value,
    current: &Value,
    app: &AppConfig,
) -> AppResult<(ControlInput, AppConfig, u64)> {
    if profile != input.profile_id
        || current["config_revision"].as_u64() != Some(input.config_revision)
    {
        return Err(AppError::conflict(
            "endpoint",
            input.endpoint_id,
            "当前配置已切换或重载，请刷新端点",
        ));
    }
    super::wire::validate_endpoint(current)?;
    let action = match input.action {
        ManagedAction::SetState { enabled } => Action::SetState {
            enabled,
            persistence: Persistence::RuntimeOnly,
        },
        ManagedAction::SetDirections { directions } => Action::SetDirections {
            directions,
            persistence: Persistence::RuntimeOnly,
        },
        ManagedAction::Restart {} => Action::Restart {},
    };
    let next = endpoints::candidate(app, profile, source, current, &action)?;
    Ok((
        ControlInput {
            endpoint_id: input.endpoint_id,
            core_instance_id: input.core_instance_id,
            expected_intent_revision: input.expected_intent_revision,
            action,
        },
        next,
        input.config_revision,
    ))
}
fn publish_confirmed(
    result: AppResult<Value>,
    publish: impl FnOnce() -> AppResult<()>,
) -> AppResult<Value> {
    // A timeout or failed reconciliation must never create a saved intent.
    let confirmed = result?;
    publish().map_err(|error| AppError {
        code: "endpoint_preferences_unsaved",
        message: format!(
            "内核已生效，但本地覆盖保存失败；重启可能恢复原配置：{}",
            error.message
        ),
        details: Some(serde_json::json!({"endpoint":confirmed,"storageError":error.details})),
    })?;
    Ok(confirmed)
}
pub async fn control(
    state: &AppState,
    input: ManagedInput,
    options: CoreIpcOptions,
) -> AppResult<Value> {
    let (profile, source) =
        active(state)?.ok_or_else(|| AppError::invalid_argument("请先选择一份配置"))?;
    let caps = crate::kernel::zero::queries::zero_capabilities(Some(options.clone())).await?;
    if !super::supports(&caps, "network_endpoint_control_v1") {
        return Err(super::unsupported("当前内核未声明端点控制能力"));
    }
    let current = super::get(&input.endpoint_id, options.clone()).await?;
    let previous = lock(state.app_config(), "app_config")?.clone();
    let (input, next, revision) = prepare(input, &profile, &source, &current, &previous)?;
    let changed = !matches!(input.action, Action::Restart {});
    super::control_params(&input, &current, &caps)?;
    let result = super::direction_transition::apply(input, current, &caps, options, revision).await;
    publish_confirmed(result, || {
        if changed {
            endpoints::store(state, next)
        } else {
            Ok(())
        }
    })
}

#[cfg(test)]
#[path = "managed_tests.rs"]
mod tests;
