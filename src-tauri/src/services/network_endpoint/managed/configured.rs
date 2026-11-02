//! State changes with imported DNS use the existing configuration transaction.
use super::*;
use tauri::{AppHandle, State};

pub(super) async fn apply(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    previous: AppConfig,
    next: AppConfig,
    input: &ControlInput,
    current: &Value,
    caps: &crate::models::gui_core::GuiZeroCapabilities,
    options: CoreIpcOptions,
) -> AppResult<Value> {
    let (id, source) = crate::configuration::local_edits::context(state.inner())?;
    let candidate = crate::services::rule_overlay::compose_candidate_with_app(
        state.inner(),
        &source,
        id.as_deref(),
        &next,
    )?;
    use crate::kernel::{adapter::KernelAdapter, zero::adapter::ZeroAdapter};
    let validation = ZeroAdapter::new()
        .validate_config(candidate.config, options.clone())
        .await?;
    crate::commands::config_workspace::ensure_validation_accepted(&validation)?;
    // Validate the actual live identity and intent again after preflight. Never
    // persist a stale click or claim an unconfirmed configuration operation.
    let observed = super::super::get(&input.endpoint_id, options.clone()).await?;
    super::super::control_params(input, &observed, caps)?;
    if observed["config_revision"] != current["config_revision"] {
        return Err(AppError::conflict(
            "endpoint",
            &input.endpoint_id,
            "配置已变化，请刷新后重试",
        ));
    }
    // Reload retains runtime overrides. First use the real acknowledged CAS
    // command, then publish matching persisted configuration and DNS together.
    let confirmed = super::super::direction_transition::apply(
        input.clone(),
        observed,
        caps,
        options.clone(),
        current["config_revision"].as_u64().unwrap(),
    )
    .await?;
    if let Err(error) =
        crate::commands::app_config::apply_candidate(app_handle, state, previous, next).await
    {
        return Err(AppError {
            code: "endpoint_dns_apply_unconfirmed",
            message: "端点状态已变更，但 DNS 配置应用未确认；请刷新后核对，未自动重放操作".into(),
            details: Some(
                serde_json::json!({"endpoint":confirmed,"cause":error.message,"details":error.details}),
            ),
        });
    }
    let result = super::super::get(&input.endpoint_id, options).await?;
    super::super::ensure_instance(&result, &input.core_instance_id)?;
    if !super::super::wire::matches_action(&result, &input.action) {
        return Err(AppError::conflict(
            "endpoint",
            &input.endpoint_id,
            "DNS 已更新，但端点意图已被其他操作改变，请刷新后核对",
        ));
    }
    Ok(result)
}
