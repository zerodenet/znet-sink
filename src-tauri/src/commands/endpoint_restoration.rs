use crate::{
    errors::{AppError, AppResult},
    models::{core::CoreIpcOptions, endpoint_restoration::*},
    services::{common::lock, core_config, endpoint_restoration, interaction_mode},
    state::app_state::AppState,
};
use tauri::{State, Window};
fn require_host_label(label: &str) -> AppResult<()> {
    if label != "main" {
        return Err(AppError {
            code: "forbidden",
            message: "端点来源恢复仅允许宿主主窗口调用".into(),
            details: None,
        });
    }
    Ok(())
}
fn options(state: &AppState) -> AppResult<CoreIpcOptions> {
    interaction_mode::require_pro_mode(state, "network_endpoints")?;
    Ok(core_config::ipc_options_from_app_config(
        &lock(state.app_config(), "app_config")?.core,
    ))
}
#[tauri::command]
pub async fn gui_endpoint_restoration_preview(
    state: State<'_, AppState>,
    window: Window,
    input: PreviewInput,
) -> AppResult<RestorationPlan> {
    require_host_label(window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    endpoint_restoration::preview(state.inner(), input, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_endpoint_restoration_apply(
    state: State<'_, AppState>,
    window: Window,
    input: ApplyInput,
) -> AppResult<RestorationReceipt> {
    require_host_label(window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    endpoint_restoration::apply(state.inner(), input, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_endpoint_restoration_reconcile(
    state: State<'_, AppState>,
    window: Window,
    operation_id: String,
) -> AppResult<RestorationReceipt> {
    require_host_label(window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    endpoint_restoration::reconcile(state.inner(), operation_id, options(state.inner())?).await
}
#[tauri::command]
pub fn gui_endpoint_restoration_receipts(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<Vec<RestorationReceipt>> {
    require_host_label(window.label())?;
    interaction_mode::require_pro_mode(state.inner(), "network_endpoints")?;
    endpoint_restoration::receipts(state.inner())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_host_main_window_can_restore_or_read_transactions() {
        assert!(super::require_host_label("main").is_ok());
        for label in ["plugin.publisher.component", "traffic-ball", "", "MAIN"] {
            assert_eq!(
                super::require_host_label(label).unwrap_err().code,
                "forbidden"
            );
        }
    }
}
