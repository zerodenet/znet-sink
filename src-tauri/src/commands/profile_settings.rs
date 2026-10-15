use crate::configuration::local_edits::{self, Edits};
use crate::errors::{AppError, AppResult};
use crate::services::common::lock;
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::{AppHandle, State};

/// Route rule edits require a host-owned fixed preview and local-only save.
/// This legacy settings entry can trigger runtime effects and cannot substitute.
fn require_legacy_settings_fields(changes: &Edits, reset: &[String]) -> AppResult<()> {
    if changes.contains_key("routeRules")
        || changes.contains_key("ruleSetReferences")
        || reset
            .iter()
            .any(|key| matches!(key.as_str(), "routeRules" | "ruleSetReferences"))
    {
        return Err(AppError {
            code: "route_editor_preview_required",
            message: "路由规则覆盖需要专用预览确认；请使用路由规则编辑器，本入口不会提交".into(),
            details: None,
        });
    }
    Ok(())
}

#[tauri::command]
pub fn profile_settings_get(state: State<'_, AppState>) -> AppResult<Value> {
    local_edits::view(state.inner())
}

#[tauri::command]
pub fn client_settings_get(state: State<'_, AppState>) -> AppResult<Value> {
    let saved = lock(state.app_config(), "app_config")?.clone();
    let (id, source) = local_edits::context(state.inner())?;
    local_edits::settings(&saved, id.as_deref(), &source)
}

#[tauri::command]
pub async fn profile_settings_apply(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    changes: Edits,
    reset: Vec<String>,
) -> AppResult<Value> {
    require_legacy_settings_fields(&changes, &reset)?;
    let _operation = state.proxy_config_operation().lock().await;
    let (active, _) = local_edits::active(state.inner())?;
    if active != profile_id {
        return Err(AppError::conflict(
            "configuration",
            "profile",
            "当前配置已切换，请重新读取后再修改",
        ));
    }
    let old = lock(state.app_config(), "app_config")?.clone();
    let next = local_edits::candidate(&old, &active, changes, &reset)?;
    if old != next {
        super::app_config::apply_candidate(app_handle, state.clone(), old, next).await?;
    }
    let mut result = local_edits::view(state.inner())?;
    result["applied"] = serde_json::json!(
        crate::services::core_process::refresh_status(state.inner())?.state
            == crate::models::core_process::CoreProcessState::Running
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_rpc_rejects_route_rule_writes_and_resets_before_any_effect() {
        let mut edits = Edits::new();
        edits.insert(
            "routeRules".into(),
            serde_json::json!({"secret":"do-not-echo"}),
        );
        for error in [
            require_legacy_settings_fields(&edits, &[]).unwrap_err(),
            require_legacy_settings_fields(&Edits::new(), &["routeRules".into()]).unwrap_err(),
            require_legacy_settings_fields(
                &Edits::from([(
                    "ruleSetReferences".into(),
                    serde_json::json!({"secret":"do-not-echo"}),
                )]),
                &[],
            )
            .unwrap_err(),
            require_legacy_settings_fields(&Edits::new(), &["ruleSetReferences".into()])
                .unwrap_err(),
        ] {
            assert_eq!(error.code, "route_editor_preview_required");
            assert!(error.details.is_none());
            assert!(!error.message.contains("do-not-echo"));
        }
        assert!(require_legacy_settings_fields(
            &Edits::from([(
                "urlTest.url".into(),
                serde_json::json!("https://example.test")
            )]),
            &["endpoints".into()]
        )
        .is_ok());
    }
}
