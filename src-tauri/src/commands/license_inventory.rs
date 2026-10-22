use crate::errors::{AppError, AppResult};
use tauri::{AppHandle, Window};
use tauri_plugin_znet_native_io::NativeIoExt;

#[tauri::command]
pub async fn app_license_inventory(app: AppHandle, window: Window) -> AppResult<serde_json::Value> {
    crate::services::native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let rust = crate::license_inventory::embedded_snapshot();
        let (native_status, native, native_reason) = match app.native_io().license_inventory() {
            Ok(receipt) if receipt.status == "available" => {
                let value = receipt.content.as_deref().and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
                match value {
                    Some(value) if value["schemaVersion"] == 1 && value["entries"].is_array() && value["texts"].is_array()
                        && value["target"]["platform"] == crate::mobile_host::snapshot().platform => ("available".to_owned(), Some(value), None),
                    _ => ("invalid".to_owned(), None, Some("packaged-inventory-schema-or-platform-mismatch".to_owned())),
                }
            }
            Ok(receipt) => (receipt.status, None, receipt.reason.or(Some("packaged-inventory-unavailable".into()))),
            Err(_) => ("unavailable".to_owned(), None, Some("packaged-inventory-unavailable".into())),
        };
        Ok(serde_json::json!({ "schemaVersion": 1, "rust": rust, "native": {"status": native_status, "inventory": native, "reason": native_reason} }))
    })
    .await
    .map_err(|_| AppError::internal("构建许可资料读取失败，请重新检查"))?
}
