use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::tool_job::{StartToolJobRequest, ToolJobId, ToolJobKind, ToolJobSnapshot};
use crate::services::{common, core_config, interaction_mode};
use crate::state::app_state::AppState;

pub const TOOL_JOB_UPDATED_EVENT: &str = "client-core:tool-job-updated";

#[tauri::command]
pub fn gui_tool_runtime_snapshot(
    state: State<'_, AppState>,
) -> crate::services::tool_jobs::ToolRuntimeSnapshot {
    state.tool_runtime().snapshot()
}

#[tauri::command]
pub fn gui_tool_job_get(
    state: State<'_, AppState>,
    job_id: ToolJobId,
) -> AppResult<ToolJobSnapshot> {
    state
        .tool_runtime()
        .get(job_id)
        .ok_or_else(|| AppError::not_found("tool_job", job_id.0.to_string()))
}

#[tauri::command]
pub fn gui_tool_job_list(
    state: State<'_, AppState>,
    kind: Option<ToolJobKind>,
) -> Vec<ToolJobSnapshot> {
    state.tool_runtime().list(kind)
}

#[tauri::command]
pub fn gui_tool_job_cancel(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    job_id: ToolJobId,
) -> AppResult<ToolJobSnapshot> {
    let job = state
        .tool_runtime()
        .cancel(job_id, common::now_unix_ms())
        .ok_or_else(|| AppError::not_found("tool_job", job_id.0.to_string()))?;
    let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, job.clone());
    Ok(job)
}

#[tauri::command]
pub fn gui_tool_job_start(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: StartToolJobRequest,
) -> AppResult<ToolJobSnapshot> {
    require_kind_available(state.inner(), request.kind)?;
    let job = state.tool_runtime().start(
        state.client_core_snapshot().scope,
        request,
        common::now_unix_ms(),
    )?;
    let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, job.clone());
    tauri::async_runtime::spawn(run(app_handle, job.clone()));
    Ok(job)
}

fn require_kind_available(state: &AppState, kind: ToolJobKind) -> AppResult<()> {
    interaction_mode::require_pro_mode(state, "diagnostic_tool_job")?;
    match kind {
        #[cfg(feature = "tool-dns")]
        ToolJobKind::DnsLookup
        | ToolJobKind::DnsCache
        | ToolJobKind::FakeIpLookup
        | ToolJobKind::FakeIpClear => Ok(()),
        #[cfg(feature = "tool-route")]
        ToolJobKind::RouteTrace => Ok(()),
        #[allow(unreachable_patterns)]
        _ => Err(AppError {
            code: "feature_disabled",
            message: "当前产品未编译此诊断工具".to_owned(),
            details: None,
        }),
    }
}

async fn run(app_handle: AppHandle, job: ToolJobSnapshot) {
    let state = app_handle.state::<AppState>();
    let Some(_permit) = state.tool_runtime().acquire(job.id).await else {
        if let Some(update) = state
            .tool_runtime()
            .finish_cancel(job.id, common::now_unix_ms())
        {
            let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
        }
        return;
    };

    let now = common::now_unix_ms();
    if now >= job.deadline_at_unix_ms {
        if let Some(update) = state.tool_runtime().timeout(job.id, now) {
            let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
        }
        return;
    }
    let Some(running) = state.tool_runtime().mark_running(job.id, now) else {
        return;
    };
    let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, running);

    let Some(mut cancellation) = state.tool_runtime().cancellation(job.id) else {
        return;
    };
    let remaining = Duration::from_millis(job.deadline_at_unix_ms.saturating_sub(now));
    let operation = execute(state.inner(), job.kind, job.params.clone());
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => {
            if let Some(update) = state.tool_runtime().complete(job.id, result, common::now_unix_ms()) {
                let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
            }
        }
        _ = tokio::time::sleep(remaining) => {
            if let Some(update) = state.tool_runtime().timeout(job.id, common::now_unix_ms()) {
                let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
            }
        }
        changed = cancellation.changed() => {
            if changed.is_ok() {
                // Configuration/core invalidation originates in AppState and
                // has no AppHandle. Publish the already-updated terminal
                // snapshot here before draining the submitted kernel request.
                if let Some(update) = state.tool_runtime().get(job.id) {
                    let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
                }
                // Keep the permit while the submitted request drains. This
                // prevents repeated cancel/start cycles from bypassing the
                // actual kernel concurrency budget.
                let _ = tokio::time::timeout(remaining, &mut operation).await;
            }
            if let Some(update) = state.tool_runtime().finish_cancel(job.id, common::now_unix_ms()) {
                let _ = app_handle.emit(TOOL_JOB_UPDATED_EVENT, update);
            }
        }
    }
}

type Operation<'a> = Pin<Box<dyn Future<Output = AppResult<Value>> + Send + 'a>>;

fn execute<'a>(state: &'a AppState, kind: ToolJobKind, params: Value) -> Operation<'a> {
    Box::pin(async move {
        let options = core_config::ipc_options_from_app_config(
            &common::lock(state.app_config(), "app_config")?.core,
        );
        let adapter = ZeroAdapter::new();
        match kind {
            #[cfg(feature = "tool-dns")]
            ToolJobKind::DnsLookup => {
                adapter
                    .dns_lookup(required_string(&params, "hostname")?, options)
                    .await
            }
            #[cfg(feature = "tool-dns")]
            ToolJobKind::DnsCache => {
                let domain = optional_string(&params, "domain")?;
                let limit = optional_u64(&params, "limit")?.map(|value| value as usize);
                adapter.dns_cache(domain, limit, options).await
            }
            #[cfg(feature = "tool-dns")]
            ToolJobKind::FakeIpLookup => {
                let domain = optional_string(&params, "domain")?;
                let ip = optional_string(&params, "ip")?;
                adapter.fakeip_lookup(domain, ip, options).await
            }
            #[cfg(feature = "tool-dns")]
            ToolJobKind::FakeIpClear => {
                let domain = optional_string(&params, "domain")?;
                let ip = optional_string(&params, "ip")?;
                serde_json::to_value(adapter.clear_fake_ip(domain, ip, options).await?)
                    .map_err(|error| AppError::internal(error.to_string()))
            }
            #[cfg(feature = "tool-route")]
            ToolJobKind::RouteTrace => {
                let port = optional_u64(&params, "port")?
                    .unwrap_or(80)
                    .try_into()
                    .map_err(|_| AppError::invalid_argument("port must be between 0 and 65535"))?;
                adapter
                    .trace_route(
                        required_string(&params, "target")?,
                        port,
                        optional_string(&params, "protocol")?,
                        optional_string(&params, "inboundTag")?,
                        options,
                    )
                    .await
            }
            #[allow(unreachable_patterns)]
            _ => Err(AppError::invalid_argument("unsupported tool job kind")),
        }
    })
}

fn required_string(params: &Value, field: &str) -> AppResult<String> {
    optional_string(params, field)?
        .ok_or_else(|| AppError::invalid_argument(format!("{field} must be a non-empty string")))
}

fn optional_string(params: &Value, field: &str) -> AppResult<Option<String>> {
    let Some(value) = params.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| AppError::invalid_argument(format!("{field} must be a string")))?
        .trim();
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn optional_u64(params: &Value, field: &str) -> AppResult<Option<u64>> {
    params
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| AppError::invalid_argument(format!("{field} must be an integer")))
        })
        .transpose()
}
