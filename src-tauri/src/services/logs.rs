use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Mutex, OnceLock},
};

use crate::errors::AppResult;
use crate::models::logs::{
    HostDiagnosticEvent, HostOutcome, LogAppend, LogEntry, LogLevel, LogPage, LogQuery, LogSource,
    PluginAction,
};
use crate::services::common::{lock, normalize_required, now_unix_ms};
use crate::services::log_store;
use crate::state::app_state::AppState;

/// Minimum log level for stderr output, controlled by `ZNET_LOG` env var.
static MIN_STDERR_LEVEL: OnceLock<LogLevel> = OnceLock::new();
static PLUGIN_HOST_EVENT_TIMES: OnceLock<Mutex<BTreeMap<String, VecDeque<u64>>>> = OnceLock::new();

fn stderr_level() -> LogLevel {
    MIN_STDERR_LEVEL
        .get_or_init(|| {
            std::env::var("ZNET_LOG")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "trace" => Some(LogLevel::Trace),
                    "debug" => Some(LogLevel::Debug),
                    "info" => Some(LogLevel::Info),
                    "warn" => Some(LogLevel::Warn),
                    "error" => Some(LogLevel::Error),
                    _ => None,
                })
                .unwrap_or(LogLevel::Info)
        })
        .clone()
}

const LEVEL_ORDER: &[LogLevel] = &[
    LogLevel::Error,
    LogLevel::Warn,
    LogLevel::Info,
    LogLevel::Debug,
    LogLevel::Trace,
];

fn level_meets(level: &LogLevel, min: &LogLevel) -> bool {
    LEVEL_ORDER.iter().position(|l| l == level) <= LEVEL_ORDER.iter().position(|l| l == min)
}

/// Write a log entry visible in production.
///
/// - Always writes to stderr if `level` meets the `ZNET_LOG` threshold.
/// - If `state` is provided, also writes to the in-memory log buffer
///   (visible in the frontend LogPanel).
pub(crate) fn znet_log(state: Option<&AppState>, level: LogLevel, message: impl Into<String>) {
    znet_log_with_fields(state, level, message.into(), None, None);
}

/// Host callsite supplies a typed event separately from all free text/fields.
pub(crate) fn znet_host_log_fields(
    state: &AppState,
    event: HostDiagnosticEvent,
    level: LogLevel,
    message: impl Into<String>,
    fields: serde_json::Value,
) {
    znet_log_with_fields(
        Some(state),
        level,
        message.into(),
        Some(fields),
        Some(event),
    );
}

fn znet_log_with_fields(
    state: Option<&AppState>,
    level: LogLevel,
    msg: String,
    fields: Option<serde_json::Value>,
    event: Option<HostDiagnosticEvent>,
) {
    let min = stderr_level();

    if level_meets(&level, &min) {
        let prefix = match level {
            LogLevel::Error => "[ZNet] ERROR",
            LogLevel::Warn => "[ZNet] WARN",
            LogLevel::Info => "[ZNet]",
            LogLevel::Debug => "[ZNet] DEBUG",
            LogLevel::Trace => "[ZNet] TRACE",
        };
        eprintln!("{prefix} {msg}");
    }

    if let Some(state) = state {
        let _ = match event {
            Some(event) => append_host_entry(state, event, level, msg, fields),
            None => append_entry(state, LogSource::App, level, msg, fields),
        };
    }
}

pub fn list(query: Option<LogQuery>) -> AppResult<LogPage> {
    let mut query = query.unwrap_or_default();
    query.limit = Some(query.limit.unwrap_or(200).clamp(1, 1_000));
    log_store::query_page(&query)
}

pub fn append(state: &AppState, input: LogAppend) -> AppResult<LogEntry> {
    let message = normalize_required(input.message, "message")?;
    append_entry(state, input.source, input.level, message, input.fields)
}

pub(crate) fn append_entry(
    state: &AppState,
    source: LogSource,
    level: LogLevel,
    message: String,
    mut fields: Option<serde_json::Value>,
) -> AppResult<LogEntry> {
    crate::services::diagnostics::strip_untrusted_marker(&mut fields);
    append_record(state, source, level, message, fields, None)
}

/// This function derives the log source from the closed host event; callers do
/// not supply a provenance JSON object or borrow authority from a source name.
pub(crate) fn append_host_entry(
    state: &AppState,
    event: HostDiagnosticEvent,
    level: LogLevel,
    message: String,
    mut fields: Option<serde_json::Value>,
) -> AppResult<LogEntry> {
    crate::services::diagnostics::strip_untrusted_marker(&mut fields);
    append_record(state, event.source(), level, message, fields, Some(event))
}

fn append_record(
    state: &AppState,
    source: LogSource,
    level: LogLevel,
    message: String,
    fields: Option<serde_json::Value>,
    diagnostic_event: Option<HostDiagnosticEvent>,
) -> AppResult<LogEntry> {
    let message = crate::kernel::redaction::text(&message);
    let fields = fields.map(|value| crate::kernel::redaction::sensitive(&value));
    let entry = LogEntry {
        diagnostic_event,
        id: state.next_record_id(),
        source,
        level,
        message,
        fields,
        occurred_at_unix_ms: now_unix_ms(),
    };

    let max_entries = lock(state.app_config(), "app_config")?.logs.max_entries;
    let mut entries = lock(state.logs(), "logs")?;
    entries.push(entry.clone());
    if entries.len() > max_entries {
        let remove_count = entries.len() - max_entries;
        entries.drain(0..remove_count);
    }
    drop(entries);
    log_store::append(&entry, max_entries)?;

    Ok(entry)
}

/// Classify an attempted SDK operation at its actual typed dispatcher. This is
/// not a classifier for component invoke action strings, and grants no authority.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn plugin_sdk_action(method: znet_plugin_sandbox::sdk::Method) -> PluginAction {
    use znet_plugin_sandbox::sdk::Method;
    match method {
        Method::SubscriptionApply => PluginAction::ManagedSubscriptionApply,
        Method::SubscriptionMetadataUpdate => PluginAction::ManagedSubscriptionMetadata,
        Method::SubscriptionSyncComplete => PluginAction::ManagedSubscriptionSync,
        Method::SubscriptionRemove => PluginAction::ManagedSubscriptionRemove,
        Method::ProtectedLoad => PluginAction::ProtectedMaterialLoad,
        _ => PluginAction::ComponentAction,
    }
}

/// Host-owned plugin execution telemetry. Never accepts a guest-provided source
/// or identity, and never records invocation payloads or SDK arguments.
pub(crate) fn plugin_host_event(
    state: &AppState,
    plugin_id: &str,
    component_id: &str,
    action: PluginAction,
    result: Result<(), &crate::errors::AppError>,
) {
    if plugin_id.is_empty()
        || component_id.is_empty()
        || plugin_id.len() > 128
        || component_id.len() > 128
    {
        return;
    }
    let now = now_unix_ms();
    let owner = format!("{plugin_id}/{component_id}");
    let mut guard = PLUGIN_HOST_EVENT_TIMES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    guard.retain(|_, times| {
        while times
            .front()
            .is_some_and(|time| now.saturating_sub(*time) >= 60_000)
        {
            times.pop_front();
        }
        !times.is_empty()
    });
    let times = guard.entry(owner).or_default();
    if times.len() >= 120 {
        return;
    }
    times.push_back(now);
    drop(guard);
    let (level, outcome, code) = match result {
        Ok(()) => (LogLevel::Info, "ok", None),
        Err(error) => (LogLevel::Error, "failed", Some(error.code)),
    };
    let _ = append_host_entry(
        state,
        HostDiagnosticEvent::Plugin {
            action,
            outcome: if result.is_ok() {
                HostOutcome::Succeeded
            } else {
                HostOutcome::Failed
            },
        },
        level,
        format!("插件操作 {action:?} {outcome}"),
        Some(serde_json::json!({
            "pluginId": plugin_id,
            "componentId": component_id,
            "action": action,
            "outcome": outcome,
            "errorCode": code,
            "origin": "host",
        })),
    );
}

pub fn clear(state: &AppState) -> AppResult<()> {
    clear_memory(state)?;
    log_store::clear()?;
    Ok(())
}

pub(crate) fn clear_memory(state: &AppState) -> AppResult<()> {
    lock(state.logs(), "logs")?.clear();
    Ok(())
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use crate::models::logs::{ProbeEvent, ProcessEvent};
    use serde_json::json;

    #[test]
    fn app_rpc_and_ordinary_core_plugin_entries_never_gain_host_classification() {
        let directory = tempfile::tempdir().unwrap();
        log_store::with_test_path(&directory.path().join("logs.jsonl"), || {
            let state = AppState::default();
            for source in [LogSource::App, LogSource::Core, LogSource::Plugin] {
                let input:LogAppend=serde_json::from_value(json!({"source":source,"level":"info",
                    "message":"core process started secret-canary",
                    "diagnosticEvent":{"kind":"process","event":"started"},
                    "fields":{"schema":"host-action-outcome.v1","origin":"host",
                        "diagnosticEvent":{"kind":"process","event":"started"},
                        "__znetHostDiagnosticV1":{"schema":"host-action-outcome.v1","action":"run","outcome":"ok"}}})).unwrap();
                let entry = append(&state, input).unwrap();
                assert!(entry.diagnostic_event.is_none());
                assert!(entry
                    .fields
                    .as_ref()
                    .unwrap()
                    .get("__znetHostDiagnosticV1")
                    .is_none());
                // The same ordinary writer is used by parsed core stderr/IPC and
                // plugin SDK logs. Source identity is insufficient for trust.
                let direct = append_entry(
                    &state,
                    source,
                    LogLevel::Info,
                    "host-looking free text".into(),
                    entry.fields,
                )
                .unwrap();
                assert!(direct.diagnostic_event.is_none());
            }
            let reopened =
                log_store::load_recent_from_path(&directory.path().join("logs.jsonl"), 20).unwrap();
            assert_eq!(reopened.len(), 6);
            assert!(reopened
                .iter()
                .all(|entry| entry.diagnostic_event.is_none()));
        });
    }
    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn diagnostic_sdk_categories_come_from_actual_typed_method_and_keep_failure_outcome() {
        use znet_plugin_sandbox::sdk::Method;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("logs.jsonl");
        log_store::with_test_path(&path, || {
            let state = AppState::default();
            let error = crate::errors::AppError::invalid_argument("secret-canary-sdk-error");
            for (method, expected) in [
                (
                    Method::SubscriptionApply,
                    PluginAction::ManagedSubscriptionApply,
                ),
                (
                    Method::SubscriptionMetadataUpdate,
                    PluginAction::ManagedSubscriptionMetadata,
                ),
                (
                    Method::SubscriptionSyncComplete,
                    PluginAction::ManagedSubscriptionSync,
                ),
                (
                    Method::SubscriptionRemove,
                    PluginAction::ManagedSubscriptionRemove,
                ),
                (Method::ProtectedLoad, PluginAction::ProtectedMaterialLoad),
                (Method::LogWrite, PluginAction::ComponentAction),
            ] {
                let action = plugin_sdk_action(method);
                assert_eq!(action, expected);
                plugin_host_event(
                    &state,
                    "fixture.diagnostic.sdk-method",
                    "component",
                    action,
                    Err(&error),
                );
                assert_eq!(
                    state
                        .logs()
                        .lock()
                        .unwrap()
                        .last()
                        .unwrap()
                        .diagnostic_event,
                    Some(HostDiagnosticEvent::Plugin {
                        action: expected,
                        outcome: HostOutcome::Failed
                    })
                );
            }
            assert_eq!(
                log_store::load_recent_from_path(&path, 10).unwrap().len(),
                6
            );
        });
    }
    #[test]
    fn actual_host_writers_attach_separate_closed_events_and_roundtrip_them() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("logs.jsonl");
        log_store::with_test_path(&path, || {
            let state = AppState::default();
            let event = HostDiagnosticEvent::Process(ProcessEvent::Started);
            let entry = append_host_entry(
                &state,
                event,
                LogLevel::Info,
                "free-secret-canary".into(),
                Some(json!({"payload":"secret-canary"})),
            )
            .unwrap();
            assert_eq!(entry.diagnostic_event, Some(event));
            assert_eq!(entry.source, LogSource::App);
            znet_host_log_fields(
                &state,
                HostDiagnosticEvent::NodeProbe(ProbeEvent::Requested),
                LogLevel::Debug,
                "target-secret-canary",
                json!({"target":"secret-canary"}),
            );
            plugin_host_event(
                &state,
                "fixture.diagnostic.writer",
                "component",
                PluginAction::ScheduledComponentAction,
                Ok(()),
            );
            let reopened = log_store::load_recent_from_path(&path, 20).unwrap();
            assert_eq!(reopened.len(), 3);
            assert_eq!(reopened[0].diagnostic_event, Some(event));
            assert!(reopened[1].diagnostic_event.is_some());
            let plugin = reopened[2].diagnostic_event.unwrap();
            assert_eq!(
                plugin,
                HostDiagnosticEvent::Plugin {
                    action: PluginAction::ScheduledComponentAction,
                    outcome: HostOutcome::Succeeded
                }
            );
            assert!(!serde_json::to_string(&plugin).unwrap().contains("canary"));
            assert!(reopened[2]
                .fields
                .as_ref()
                .unwrap()
                .get("__znetHostDiagnosticV1")
                .is_none());
        });
    }
}
