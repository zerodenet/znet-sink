use crate::errors::{AppError, AppResult};
use crate::models::settings_transfer::{TransferExport, TransferPreview, TransferReceipt};
use crate::services::settings_transfer;
use crate::state::app_state::AppState;
use tauri::{State, Window};

fn require_host(state: &AppState, window: &Window) -> AppResult<()> {
    require_host_label(window.label())?;
    crate::services::interaction_mode::require_pro_mode(state, "settings.transfer")
}

fn require_host_label(label: &str) -> AppResult<()> {
    if label != "main" {
        return Err(AppError {
            code: "forbidden",
            message: "设置迁移仅允许宿主主窗口调用".into(),
            details: None,
        });
    }
    Ok(())
}

#[tauri::command]
pub fn settings_transfer_export(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<TransferExport> {
    require_host(state.inner(), &window)?;
    let current = crate::services::common::lock(state.app_config(), "app_config")?.clone();
    settings_transfer::export(&current)
}
#[tauri::command]
pub async fn settings_transfer_preview(
    state: State<'_, AppState>,
    window: Window,
    content: String,
) -> AppResult<TransferPreview> {
    require_host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    settings_transfer::preview(state.inner(), window.label(), &content)
}
#[tauri::command]
pub async fn settings_transfer_commit(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<TransferReceipt> {
    require_host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    settings_transfer::commit(state.inner(), window.label(), &plan_id)
}
#[tauri::command]
pub async fn settings_transfer_status(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<TransferReceipt> {
    require_host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    settings_transfer::status(state.inner(), window.label(), &plan_id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn plugin_and_secondary_window_labels_cannot_invoke_host_settings_transfer() {
        assert!(super::require_host_label("main").is_ok());
        for label in ["plugin.publisher.component", "traffic-ball", "", "MAIN"] {
            assert_eq!(
                super::require_host_label(label).unwrap_err().code,
                "forbidden"
            );
        }
    }
}
