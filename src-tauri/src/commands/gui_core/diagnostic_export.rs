//! Desktop delivery of the same fixed public projection used by native export.
use super::GuiDiagnosticExport;
use crate::{
    errors::{AppError, AppResult},
    services::{common::now_unix_ms, diagnostic_storage, diagnostics},
    state::app_state::AppState,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const FILE_NAME: &str = "diagnostics.json";

pub(super) fn admit(label: &str, mobile: bool) -> AppResult<()> {
    crate::services::native_documents::require_host(label)?;
    if mobile {
        return Err(AppError::invalid_argument(
            "移动端请使用系统保存或分享原诊断快照",
        ));
    }
    Ok(())
}

pub(super) fn export(state: &AppState, plan_id: &str) -> AppResult<GuiDiagnosticExport> {
    export_with_root(state, plan_id, crate::services::data_dir)
}

fn export_with_root(
    state: &AppState,
    plan_id: &str,
    resolve_root: impl FnOnce() -> AppResult<PathBuf>,
) -> AppResult<GuiDiagnosticExport> {
    export_with_delivery(state, plan_id, resolve_root, write_prepared)
}

fn export_with_delivery(
    state: &AppState,
    plan_id: &str,
    resolve_root: impl FnOnce() -> AppResult<PathBuf>,
    deliver: impl FnOnce(&Path, &[u8]) -> AppResult<GuiDiagnosticExport>,
) -> AppResult<GuiDiagnosticExport> {
    diagnostic_storage::with_storage_lock(|| {
        let original = state.client_core_snapshot();
        // The plan's original generation is checked by take. Retain this
        // generation across IO too; a content digest can recur after A->B->A.
        let bytes = diagnostics::take(state, plan_id)?;
        let root = resolve_root()?;
        state.ensure_client_scope_generation(&original.scope, original.scope_generation.0)?;
        let receipt = deliver(&root, &bytes)?;
        state
            .ensure_client_scope_generation(&original.scope, original.scope_generation.0)
            .map_err(|_| delivery_unknown())?;
        Ok(receipt)
    })
}

fn write_prepared(root: &Path, bytes: &[u8]) -> AppResult<GuiDiagnosticExport> {
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("诊断快照超过2MiB，未写入文件"));
    }
    let now = now_unix_ms();
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| AppError::internal("无法创建诊断目录标识"))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let parent = root.join("diagnostics");
    let directory = parent.join(format!("znet-sink-diagnostics-{now}-{suffix}"));
    // Public errors deliberately omit filesystem errors, which can contain
    // private paths. Never fall back to raw logs on any projection/IO failure.
    fs::create_dir_all(&parent).map_err(|_| delivery_unknown())?;
    fs::create_dir(&directory).map_err(|_| delivery_unknown())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(FILE_NAME))
        .map_err(|_| delivery_unknown())?;
    file.write_all(bytes).map_err(|_| delivery_unknown())?;
    file.sync_all().map_err(|_| delivery_unknown())?;
    Ok(GuiDiagnosticExport {
        directory: directory.to_string_lossy().into_owned(),
        files: vec![FILE_NAME.into()],
        created_at_unix_ms: now,
    })
}

fn delivery_unknown() -> AppError {
    AppError::internal(
        "诊断文件写入未确认，可能存在未完成文件；请检查本地诊断目录并重新预览，不会重放原计划",
    )
}

#[cfg(test)]
#[path = "diagnostic_export_tests.rs"]
mod tests;
