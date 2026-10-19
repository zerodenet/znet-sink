//! Host-owned payload preparation. Native URI/path grants never cross into the UI.
use crate::{
    errors::{AppError, AppResult},
    models::{
        native_documents::{
            DocumentCapabilities, DocumentDelivery, DocumentPurpose, DocumentReceipt,
        },
        proxy_config::ProxyConfigProfile,
    },
    services::common::lock,
    state::app_state::AppState,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};
use tauri_plugin_znet_native_io::{
    Error, ExportOwnership, ExportPurpose, ImportPurpose, NativeIoExt, PreparedExport, Receipt,
    ReceiptStatus, CONFIGURATION_MAX_BYTES, SETTINGS_MAX_BYTES,
};

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
fn operation_id() -> String {
    format!(
        "document-{}-{}",
        super::common::now_unix_ms(),
        NEXT_OPERATION.fetch_add(1, Ordering::Relaxed)
    )
}
fn policy_error(_: Error) -> AppError {
    AppError::invalid_argument("文件操作的用途、归属、确认或容量不符合宿主策略")
}
pub(crate) fn capabilities(app: &AppHandle) -> DocumentCapabilities {
    let available = app.native_io().source_available();
    DocumentCapabilities {
        platform: crate::mobile_host::snapshot().platform,
        available,
        minimum_android_api: 24,
        adaptation_state: if available {
            "implemented-awaiting-device-validation"
        } else {
            "not-applicable"
        },
        reason: (!available).then_some(
            "此接口使用 Android/iOS 系统文件与分享适配器；桌面继续使用现有系统文件流程。",
        ),
        import_maximum_bytes: CONFIGURATION_MAX_BYTES,
        settings_maximum_bytes: SETTINGS_MAX_BYTES,
    }
}
pub(crate) fn require_host(label: &str) -> AppResult<()> {
    if label != "main" {
        return Err(AppError::invalid_argument(
            "仅宿主主页面可以执行系统文档操作",
        ));
    }
    Ok(())
}
fn convert(id: String, result: Result<Receipt, Error>) -> AppResult<DocumentReceipt> {
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(Error::BridgeResultUnknown | Error::InvalidReceipt(_)) => {
            return Ok(DocumentReceipt {
                operation_id: id,
                status: ReceiptStatus::ResultUnknown,
                content: None,
                bytes_read: None,
                reason: Some("系统回执未确认；请先检查目标，操作不会自动重放".into()),
            })
        }
        Err(Error::Unavailable) => {
            return Err(AppError::invalid_argument("当前平台没有原生文档适配器"))
        }
        Err(Error::Busy) => {
            return Err(AppError::invalid_argument(
                "另一个系统文档操作尚未结束，请先完成该操作",
            ))
        }
        Err(error) => return Err(policy_error(error)),
    };
    let content = receipt
        .read_bytes()
        .map_err(policy_error)?
        .map(|bytes| String::from_utf8(bytes))
        .transpose()
        .map_err(|_| AppError::invalid_argument("系统文件必须使用 UTF-8 编码；原草稿保留"))?;
    Ok(DocumentReceipt {
        operation_id: receipt.operation_id,
        status: receipt.status,
        content,
        bytes_read: receipt.bytes_read,
        reason: receipt.reason,
    })
}
pub(crate) fn pick(app: &AppHandle, purpose: DocumentPurpose) -> AppResult<DocumentReceipt> {
    let id = operation_id();
    let (purpose, maximum) = match purpose {
        DocumentPurpose::Configuration => (ImportPurpose::Configuration, CONFIGURATION_MAX_BYTES),
        DocumentPurpose::Settings => (ImportPurpose::Settings, SETTINGS_MAX_BYTES),
    };
    convert(id.clone(), app.native_io().pick_read(&id, purpose, maximum))
}
pub(crate) fn deliver_prepared(
    app: &AppHandle,
    export: PreparedExport,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    let id = operation_id();
    let result = match delivery {
        DocumentDelivery::Save => app.native_io().save(&id, &export),
        DocumentDelivery::Share => app.native_io().share(&id, &export),
    };
    convert(id, result)
}
fn profile_export(
    profile: &ProxyConfigProfile,
    expected: u64,
    acknowledged: bool,
) -> AppResult<PreparedExport> {
    if profile.managed_source.is_some() {
        return Err(AppError::invalid_argument(
            "托管配置不能导出到普通系统文件，请通过来源插件管理",
        ));
    }
    if profile.updated_at_unix_ms != expected {
        return Err(AppError::conflict(
            "proxy_config",
            &profile.id,
            "配置已更新，请刷新并重新确认导出",
        ));
    }
    let content = profile.content.as_ref().ok_or_else(|| {
        AppError::invalid_argument("此配置没有可导出的本地正文，请通过系统文件选择并导入材料")
    })?;
    let bytes =
        serde_json::to_vec_pretty(content).map_err(|_| AppError::internal("配置导出序列化失败"))?;
    PreparedExport::new(
        ExportPurpose::OrdinaryConfiguration,
        ExportOwnership::OrdinaryProfile,
        "znet-profile.json",
        "application/json",
        bytes,
        acknowledged,
    )
    .map_err(policy_error)
}
pub(crate) fn export_profile(
    app: &AppHandle,
    id: &str,
    expected: u64,
    delivery: DocumentDelivery,
    acknowledged: bool,
) -> AppResult<DocumentReceipt> {
    let state = app.state::<AppState>();
    let prepared = {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let profile = profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| AppError::not_found("proxy_config", id))?;
        profile_export(profile, expected, acknowledged)?
    };
    deliver_prepared(app, prepared, delivery)
}
pub(crate) fn export_settings(
    app: &AppHandle,
    expected: &str,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    let state = app.state::<AppState>();
    let exported = super::settings_transfer::export(&*lock(state.app_config(), "app_config")?)?;
    if exported.expected_revision != expected {
        return Err(AppError::conflict(
            "settings",
            "",
            "设置已更新，请重新预览导出范围",
        ));
    }
    let prepared = PreparedExport::new(
        ExportPurpose::Settings,
        ExportOwnership::HostGenerated,
        "znet-settings.json",
        "application/json",
        exported.document.into_bytes(),
        false,
    )
    .map_err(policy_error)?;
    deliver_prepared(app, prepared, delivery)
}
pub(crate) fn export_diagnostics(
    app: &AppHandle,
    delivery: DocumentDelivery,
    plan_id: Option<String>,
) -> AppResult<DocumentReceipt> {
    let state = app.state::<AppState>();
    let snapshot = state.client_core_snapshot();
    // Legacy explicit export calls retain compatibility. The new mobile flow
    // always supplies a host-prepared preview ID and never resnapshots here.
    let id = match plan_id {
        Some(id) => id,
        None => super::diagnostics::preview(state.inner())?.plan_id,
    };
    let bytes = super::diagnostics::take(state.inner(), &id)?;
    let prepared = PreparedExport::new(
        ExportPurpose::RedactedDiagnostic,
        ExportOwnership::HostGenerated,
        "znet-diagnostics.json",
        "application/json",
        bytes,
        false,
    )
    .map_err(policy_error)?;
    state.ensure_client_scope_generation(&snapshot.scope, snapshot.scope_generation.0)?;
    let result = deliver_prepared(app, prepared, delivery);
    state
        .ensure_client_scope_generation(&snapshot.scope, snapshot.scope_generation.0)
        .map_err(|_| {
            AppError::internal(
                "交付期间配置或实例代次变化；系统可能已处理原操作，结果未确认，不会自动重试",
            )
        })?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> ProxyConfigProfile {
        ProxyConfigProfile {
            policy_selections: Default::default(),
            id: "local".into(),
            name: "Local".into(),
            kernel: "zero".into(),
            format: "json".into(),
            path: None,
            content: Some(serde_json::json!({"outbounds":[]})),
            active: false,
            managed_source: None,
            updated_at_unix_ms: 17,
            capabilities: Default::default(),
        }
    }
    #[test]
    fn export_requires_current_material_and_credential_warning() {
        assert!(profile_export(&profile(), 17, false).is_err());
        assert!(profile_export(&profile(), 16, true).is_err());
        assert!(profile_export(&profile(), 17, true).is_ok());
        let mut missing = profile();
        missing.content = None;
        assert!(profile_export(&missing, 17, true).is_err());
    }
    #[test]
    fn lost_or_invalid_native_acknowledgement_stays_unknown() {
        for error in [
            Error::BridgeResultUnknown,
            Error::InvalidReceipt("operation_identity_mismatch"),
        ] {
            let receipt = convert("original".into(), Err(error)).unwrap();
            assert_eq!(receipt.operation_id, "original");
            assert_eq!(receipt.status, ReceiptStatus::ResultUnknown);
            assert!(receipt.content.is_none());
        }
    }
    #[test]
    fn guest_window_has_no_document_authority() {
        assert!(require_host("plugin-guest").is_err());
        assert!(require_host("main").is_ok());
    }
    #[test]
    fn managed_material_cannot_escape_via_an_ordinary_export() {
        let mut managed = profile();
        managed.managed_source = Some(crate::models::subscription::ManagedSubscriptionSource {
            plugin_id: "trusted.publisher".into(),
            provider_id: "provider".into(),
            remote_subscription_id: "remote".into(),
            revision: None,
            source_name: None,
        });
        assert!(profile_export(&managed, 17, true).is_err());
    }
}
