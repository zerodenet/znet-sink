//! Fixed authoritative log selections. No caller-provided body or host marker
//! can become public diagnostic text; search is used locally and never exported.
use crate::{
    errors::{AppError, AppResult},
    models::{
        logs::{LogEntry, LogLevel, LogSource},
        native_documents::{DiagnosticScope, DocumentDelivery, DocumentReceipt},
    },
    services::{common::now_unix_ms, diagnostics},
    state::app_state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

const MAX_SELECTION: usize = 2000;
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedLog {
    pub id: String,
    pub occurred_at_unix_ms: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogExportRequest {
    pub items: Vec<SelectedLog>,
    pub source: Option<LogSource>,
    pub level: Option<LogLevel>,
    pub search_applied: bool,
    pub from_unix_ms: Option<String>,
    pub until_unix_ms: Option<String>,
    pub partial: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogExportPreview {
    pub plan_id: String,
    pub created_at_unix_ms: String,
    pub expires_at_unix_ms: String,
    pub bytes: u32,
    pub count: usize,
    pub classified_count: usize,
    pub unclassified_count: usize,
    pub partial: bool,
    pub scope: DiagnosticScope,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogDelivery {
    Copy,
    Save,
    Share,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogExportReceipt {
    pub status: &'static str,
    pub document: Option<DocumentReceipt>,
}
fn invalid() -> AppError {
    AppError::invalid_argument("日志选择或过滤范围无效；保留列表，请重新选择")
}
fn conflict() -> AppError {
    AppError::conflict(
        "log_export",
        "",
        "原日志身份或过滤结果已变化，请刷新并重新预览",
    )
}
fn decimal(text: &str) -> AppResult<u64> {
    let value = text.parse::<u64>().map_err(|_| invalid())?;
    if value.to_string() != text {
        return Err(invalid());
    }
    Ok(value)
}
fn selection(request: &LogExportRequest, rows: Vec<LogEntry>) -> AppResult<Vec<LogEntry>> {
    if request.items.is_empty() || request.items.len() > MAX_SELECTION {
        return Err(invalid());
    }
    let from = request.from_unix_ms.as_deref().map(decimal).transpose()?;
    let until = request.until_unix_ms.as_deref().map(decimal).transpose()?;
    if from.zip(until).is_some_and(|(a, b)| a > b) {
        return Err(invalid());
    }
    let mut actual: BTreeMap<_, _> = rows.into_iter().map(|row| (row.id, row)).collect();
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for identity in &request.items {
        let id = decimal(&identity.id)?;
        let at = decimal(&identity.occurred_at_unix_ms)?;
        if !seen.insert(id) {
            return Err(invalid());
        }
        let row = actual.remove(&id).ok_or_else(conflict)?;
        if row.occurred_at_unix_ms != at
            || request
                .source
                .as_ref()
                .is_some_and(|source| source != &row.source)
            || request
                .level
                .as_ref()
                .is_some_and(|level| level != &row.level)
            || from.is_some_and(|from| at < from)
            || until.is_some_and(|until| at > until)
        {
            return Err(conflict());
        }
        // Search membership is the original visible ID set, never re-run against
        // newly appended records or reinterpreted using another presentation layer.
        selected.push(row);
    }
    Ok(selected)
}
pub(crate) fn preview(state: &AppState, request: LogExportRequest) -> AppResult<LogExportPreview> {
    let snapshot = state.client_core_snapshot();
    let scope_generation = snapshot.scope_generation.0;
    let scope = snapshot.scope;
    let now = now_unix_ms();
    let ids = request
        .items
        .iter()
        .map(|item| decimal(&item.id))
        .collect::<AppResult<BTreeSet<_>>>()?;
    if ids.is_empty() || request.items.len() > MAX_SELECTION {
        return Err(invalid());
    }
    let rows = super::log_store::read_selection(&ids)?;
    let selected = selection(&request, rows)?;
    let logs = selected.iter().map(diagnostics::log).collect::<Vec<_>>();
    let classified = logs
        .iter()
        .filter(|row| !row["safeMessage"].is_null())
        .count();
    let public_scope = DiagnosticScope {
        profile_id: scope.profile_id.as_ref().map(|id| id.0.clone()),
        config_revision: scope.config_revision.0.to_string(),
        client_instance_id: scope.core_instance_id.0.to_string(),
    };
    let document = json!({"schemaVersion":"znet.mobile-diagnostics.v3","selectionKind":"logs","createdAtUnixMs":now.to_string(),"scope":public_scope,"historyScope":"bounded_client_storage_across_instances","partial":request.partial,"filter":{"source":request.source,"level":request.level,"searchApplied":request.search_applied,"searchBasis":"fixed_visible_ids","fromUnixMs":request.from_unix_ms,"untilUnixMs":request.until_unix_ms},"logs":logs,"omissions":[{"category":"rawMessagesAndFields","count":selected.len(),"reason":"原日志正文、任意字段与搜索原文排除；仅封闭宿主分类重建安全正文"},{"category":"unclassifiedMessages","count":selected.len()-classified,"reason":"无匹配可信宿主分类，仅保留时间、来源、级别和ID"},{"category":"framePayloadsAndTaskBodies","count":null,"reason":"此文件仅包含固定日志选择；不含帧payload、配置、凭据或任务正文"}]});
    let bytes = diagnostics::bounded_document(&document)?;
    let length = bytes.len() as u32;
    let (plan_id, expires) =
        diagnostics::store_selected(state, scope, scope_generation, bytes, now)?;
    Ok(LogExportPreview {
        plan_id,
        created_at_unix_ms: now.to_string(),
        expires_at_unix_ms: expires.to_string(),
        bytes: length,
        count: selected.len(),
        classified_count: classified,
        unclassified_count: selected.len() - classified,
        partial: request.partial,
        scope: public_scope,
    })
}
pub(crate) fn deliver(
    app: &AppHandle,
    id: &str,
    delivery: LogDelivery,
) -> AppResult<LogExportReceipt> {
    let state = app.state::<AppState>();
    let snapshot = state.client_core_snapshot();
    let scope_generation = snapshot.scope_generation.0;
    let scope = snapshot.scope;
    let bytes = diagnostics::take_selected(state.inner(), id)?;
    state.ensure_client_scope_generation(&scope, scope_generation)?;
    let result = (|| match delivery {
        LogDelivery::Copy => {
            let permission =
                znet_client_core::capability::Permission::new("platform.clipboard-write", "text");
            let lease = znet_client_capabilities::host::operation(
                state.capabilities(),
                "builtin.log-export",
                permission.clone(),
                1,
                bytes.len().max(1),
                std::time::Duration::from_secs(15),
            )
            .map_err(|_| AppError::internal("剪贴板操作未获宿主能力许可"))?;
            let byte_count = bytes.len();
            let text = String::from_utf8(bytes).map_err(|_| invalid())?;
            lease
                .execute(&permission, || {
                    app.clipboard()
                        .write_text(text)
                        .map(|()| ((), byte_count))
                        .map_err(|_| znet_client_core::capability::Error::Transport)
                })
                .and_then(|resource| resource.take(&lease))
                .map_err(|_| {
                    AppError::internal("系统剪贴板写入未确认；不会自动重试，原列表保留")
                })?;
            Ok(LogExportReceipt {
                status: "copied",
                document: None,
            })
        }
        LogDelivery::Save | LogDelivery::Share => {
            let prepared = tauri_plugin_znet_native_io::PreparedExport::new(
                tauri_plugin_znet_native_io::ExportPurpose::RedactedDiagnostic,
                tauri_plugin_znet_native_io::ExportOwnership::HostGenerated,
                "znet-redacted-logs.json",
                "application/json",
                bytes,
                false,
            )
            .map_err(|_| invalid())?;
            let receipt = super::native_documents::deliver_prepared(
                app,
                prepared,
                if matches!(delivery, LogDelivery::Save) {
                    DocumentDelivery::Save
                } else {
                    DocumentDelivery::Share
                },
            )?;
            Ok(LogExportReceipt {
                status: "system-receipt",
                document: Some(receipt),
            })
        }
    })();
    state
        .ensure_client_scope_generation(&scope, scope_generation)
        .map_err(|_| {
            AppError::internal(
                "交付期间配置或实例变化；系统可能已处理原操作，结果未确认，不会自动重试",
            )
        })?;
    result
}

#[cfg(test)]
#[path = "log_export_tests.rs"]
mod tests;
