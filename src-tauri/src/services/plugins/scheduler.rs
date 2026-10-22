//! Persistent plugin scheduling intent. The host performs bounded invocations;
//! plugins do not receive resident threads or exact-time guarantees.

use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tauri::Manager as _;
use znet_plugin_sandbox::contract::{Capability, Request};

pub(super) mod attempt;
pub(super) use attempt::Owner;
use attempt::{Attempt, Completion, Phase};

const MAX_BYTES: usize = 256 * 1024;
const MAX_TASKS: usize = 256;
const MIN_INTERVAL_SECONDS: u64 = 300;
const MAX_INTERVAL_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Task {
    pub plugin_id: String,
    pub component_id: String,
    pub task_id: String,
    pub action: String,
    pub interval_seconds: u64,
    pub next_run_unix_ms: u64,
    #[serde(default)]
    pub failure_count: u8,
    /// Opaque intent revision; identical values saved again are a new target.
    /// Empty only for legacy records, never assigned to a newly saved intent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt: Option<Attempt>,
}

#[derive(Clone)]
pub(super) struct DueTask {
    pub(super) task: Task,
    pub(super) installation_generation: u64,
    pub(super) review: Review,
    _flight: Arc<attempt::Flight>,
}
impl DueTask {
    pub(super) fn attempt_id(&self) -> AppResult<&str> {
        self.task
            .last_attempt
            .as_ref()
            .map(|attempt| attempt.id.as_str())
            .ok_or_else(stale)
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    #[serde(default = "schema")]
    schema_version: u32,
    #[serde(default)]
    tasks: BTreeMap<String, Task>,
}

fn schema() -> u32 {
    1
}
fn path(root: &Path) -> std::path::PathBuf {
    root.join("schedules.json")
}
fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
fn key(plugin_id: &str, component_id: &str, task_id: &str) -> String {
    format!("{plugin_id}/{component_id}/{task_id}")
}

fn load(root: &Path) -> AppResult<Registry> {
    fs::create_dir_all(root).map_err(io::failure)?;
    let file = match File::open(path(root)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Registry {
                schema_version: schema(),
                ..Registry::default()
            })
        }
        Err(error) => return Err(io::failure(error)),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io::failure)?;
    let registry: Registry = serde_json::from_slice(&bytes).map_err(io::failure)?;
    if bytes.len() > MAX_BYTES
        || registry.schema_version != schema()
        || registry.tasks.len() > MAX_TASKS
        || registry.tasks.iter().any(|(stored_key, task)| {
            stored_key != &key(&task.plugin_id, &task.component_id, &task.task_id)
                || !safe_id(&task.plugin_id)
                || !safe_id(&task.component_id)
                || !safe_id(&task.task_id)
                || !safe_id(&task.action)
                || task.failure_count > 16
                || task
                    .last_attempt
                    .as_ref()
                    .is_some_and(|attempt| !attempt.valid())
                || (!task.revision.is_empty()
                    && (task.revision.len() != 48
                        || !task
                            .revision
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))))
                || !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&task.interval_seconds)
        })
    {
        return Err(AppError::invalid_argument("插件调度状态格式无效"));
    }
    Ok(registry)
}

fn save(root: &Path, registry: &Registry) -> AppResult<()> {
    save_checked(root, registry, &|| Ok(()))
}
fn save_checked(
    root: &Path,
    registry: &Registry,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    if registry.tasks.len() > MAX_TASKS {
        return Err(AppError::invalid_argument(
            "插件调度数量已达 256 项上限；原有任务保留",
        ));
    }
    let bytes = serde_json::to_vec(registry).map_err(io::failure)?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件调度状态超过容量限制"));
    }
    let _history = super::uninstall::storage::writer(root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    check()?;
    temporary
        .persist(path(root))
        .map_err(|error| io::failure(error.error))?;
    #[cfg(unix)]
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(io::failure)?;
    Ok(())
}

impl Host {
    #[cfg(test)]
    pub(super) fn schedule_put(
        &self,
        plugin_id: &str,
        component_id: &str,
        task_id: String,
        action: String,
        interval_seconds: u64,
    ) -> AppResult<Task> {
        self.schedule_put_checked(
            plugin_id,
            component_id,
            task_id,
            action,
            interval_seconds,
            &|| Ok(()),
        )
    }
    pub(super) fn schedule_put_checked(
        &self,
        plugin_id: &str,
        component_id: &str,
        task_id: String,
        action: String,
        interval_seconds: u64,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<Task> {
        if !safe_id(&task_id)
            || !safe_id(&action)
            || !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&interval_seconds)
        {
            return Err(AppError::invalid_argument(
                "插件后台任务名称、动作或间隔无效（间隔需为 5 分钟至 7 天）",
            ));
        }
        let _operation = self.operation_guard()?;
        check()?;
        self.ensure_installation_current(plugin_id)?;
        let mut registry = load(&self.root()?)?;
        let task = Task {
            plugin_id: plugin_id.into(),
            component_id: component_id.into(),
            task_id,
            action,
            interval_seconds,
            next_run_unix_ms: crate::services::common::now_unix_ms()
                .saturating_add(interval_seconds.saturating_mul(1000)),
            failure_count: 0,
            revision: attempt::random_id()?,
            last_attempt: None,
        };
        registry
            .tasks
            .insert(key(plugin_id, component_id, &task.task_id), task.clone());
        save_checked(&self.root()?, &registry, check)?;
        Ok(task)
    }
    pub(super) fn schedule_list(
        &self,
        plugin_id: &str,
        component_id: &str,
    ) -> AppResult<Vec<Task>> {
        let _operation = self.operation_guard()?;
        Ok(load(&self.root()?)?
            .tasks
            .into_values()
            .filter(|task| task.plugin_id == plugin_id && task.component_id == component_id)
            .collect())
    }
    pub(super) fn schedule_delete_checked(
        &self,
        plugin_id: &str,
        component_id: &str,
        task_id: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<()> {
        if !safe_id(task_id) {
            return Err(AppError::invalid_argument("插件调度任务标识无效"));
        }
        let _operation = self.operation_guard()?;
        check()?;
        self.ensure_installation_current(plugin_id)?;
        let mut registry = load(&self.root()?)?;
        if registry
            .tasks
            .remove(&key(plugin_id, component_id, task_id))
            .is_some()
        {
            save_checked(&self.root()?, &registry, check)?;
        }
        Ok(())
    }

    pub(super) fn remove_component_schedules(
        &self,
        plugin_id: &str,
        component_id: &str,
    ) -> AppResult<()> {
        let mut registry = load(&self.root()?)?;
        let before = registry.tasks.len();
        registry
            .tasks
            .retain(|_, task| task.plugin_id != plugin_id || task.component_id != component_id);
        if registry.tasks.len() != before {
            save(&self.root()?, &registry)?;
        }
        Ok(())
    }
    pub(super) fn take_due(&self, manager: &Manager, now: u64) -> AppResult<Vec<DueTask>> {
        self.take_due_limited(manager, now, usize::MAX)
    }
    pub(super) fn take_due_limited(
        &self,
        manager: &Manager,
        now: u64,
        maximum: usize,
    ) -> AppResult<Vec<DueTask>> {
        let _operation = self.operation_guard()?;
        let schedule_permission = Request {
            capability: Capability::TasksSchedule,
            scope: "self".into(),
        };
        let mut enabled: BTreeSet<_> = self
            .state
            .lock()
            .unwrap()
            .loaded
            .values()
            .filter(|loaded| {
                loaded.consent.enabled
                    && loaded.blocked.is_none()
                    && loaded.consent.grants.contains(&schedule_permission)
            })
            .map(|loaded| {
                (
                    loaded.component.manifest().plugin_id.clone(),
                    loaded.component.manifest().component_id.clone(),
                )
            })
            .collect();
        enabled.retain(|(plugin, _)| self.ensure_installation_current(plugin).is_ok());
        let generations: BTreeMap<_, _> = self
            .state
            .lock()
            .unwrap()
            .loaded
            .values()
            .map(|l| {
                (
                    (
                        l.component.manifest().plugin_id.clone(),
                        l.component.manifest().component_id.clone(),
                    ),
                    l.installation_generation,
                )
            })
            .collect();
        let mut registry = load(&self.root()?)?;
        let policies: BTreeMap<_, _> = manager
            .component_snapshots()
            .into_iter()
            .filter(|row| row.enabled && !row.retired)
            .map(|row| (row.key.clone(), row))
            .collect();
        let host_generation = self.platform.review_generation();
        if let Some(reason) = self.platform.execution_restriction() {
            return Err(super::platform::unavailable(reason));
        }
        let mut due = Vec::new();
        let mut changed = false;
        for task in registry.tasks.values_mut() {
            let component_key = format!("{}/{}", task.plugin_id, task.component_id);
            let review = policies.get(&component_key).map(|policy| Review {
                key: component_key.clone(),
                identity: policy.identity.clone(),
                registration: policy.registration,
                revision: policy.revision,
                host_generation: host_generation.clone(),
            });
            if let Some(attempt) = task.last_attempt.as_mut() {
                changed |= attempt.recover(&self.scheduler, review.as_ref(), now)?;
            }
            if due.len() < maximum
                && task.next_run_unix_ms <= now
                && enabled.contains(&(task.plugin_id.clone(), task.component_id.clone()))
                && !task
                    .last_attempt
                    .as_ref()
                    .is_some_and(Attempt::blocks_dispatch)
            {
                let Some(review) = review else {
                    continue;
                };
                let installation_generation = *generations
                    .get(&(task.plugin_id.clone(), task.component_id.clone()))
                    .ok_or_else(stale)?;
                let attempt = Attempt::queued(now, &review)?;
                let flight = self.scheduler.hold(&attempt.id)?;
                task.last_attempt = Some(attempt);
                task.next_run_unix_ms =
                    now.saturating_add(task.interval_seconds.saturating_mul(1000));
                due.push(DueTask {
                    task: task.clone(),
                    installation_generation,
                    review,
                    _flight: flight,
                });
                changed = true;
            }
        }
        if changed {
            save_checked(&self.root()?, &registry, &|| {
                self.platform
                    .check_review_generation(host_generation.as_deref())?;
                for item in &due {
                    self.current_review(manager, &item.review)?;
                }
                Ok(())
            })?;
        }
        Ok(due)
    }

    /// Caller holds the original operation guard. Never rebind queued work to
    /// a newer registration, grant, configuration, native owner or task intent.
    pub(super) fn checked_scheduled_due(
        &self,
        manager: &Manager,
        due: &DueTask,
    ) -> AppResult<znet_client_core::capability::ComponentSnapshot> {
        let task = &due.task;
        if due.review.key != format!("{}/{}", task.plugin_id, task.component_id) {
            return Err(stale());
        }
        let binding = self.current_review(manager, &due.review)?;
        if !binding.enabled {
            return Err(stale());
        }
        self.ensure_installation_current(&task.plugin_id)?;
        if !self
            .state
            .lock()
            .unwrap()
            .loaded
            .get(&due.review.key)
            .is_some_and(|loaded| {
                loaded.installation_generation == due.installation_generation
                    && loaded.consent.enabled
                    && loaded.blocked.is_none()
                    && loaded.consent.grants.contains(&Request {
                        capability: Capability::TasksSchedule,
                        scope: "self".into(),
                    })
            })
        {
            return Err(stale());
        }
        let registry = load(&self.root()?)?;
        if !registry
            .tasks
            .get(&key(&task.plugin_id, &task.component_id, &task.task_id))
            .is_some_and(|stored| {
                stored.revision == task.revision
                    && stored.action == task.action
                    && stored.interval_seconds == task.interval_seconds
                    && stored
                        .last_attempt
                        .as_ref()
                        .is_some_and(|attempt| attempt.matches(due).unwrap_or(false))
            })
        {
            return Err(stale());
        }
        Ok(binding)
    }

    /// Caller holds the owning operation guard. Persist before crossing into VM.
    pub(super) fn mark_scheduled_dispatch(
        &self,
        manager: &Manager,
        due: &DueTask,
        now: u64,
    ) -> AppResult<()> {
        self.checked_scheduled_due(manager, due)?;
        let mut registry = load(&self.root()?)?;
        let task = registry
            .tasks
            .get_mut(&key(
                &due.task.plugin_id,
                &due.task.component_id,
                &due.task.task_id,
            ))
            .ok_or_else(stale)?;
        let attempt = task.last_attempt.as_mut().ok_or_else(stale)?;
        if attempt.state != Phase::Queued {
            return Err(stale());
        }
        attempt.state = Phase::Running;
        attempt.dispatch_at_unix_ms = Some(now.to_string());
        attempt.observed_at_unix_ms = now.to_string();
        save_checked(&self.root()?, &registry, &|| {
            self.checked_scheduled_due(manager, due).map(|_| ())
        })
    }

    #[cfg(test)]
    pub(super) fn record_schedule_result(
        &self,
        task: &Task,
        installation_generation: u64,
        succeeded: bool,
        now: u64,
    ) -> AppResult<()> {
        self.record_schedule_result_checked(
            task,
            installation_generation,
            Completion::Known(succeeded),
            now,
            &|| Ok(()),
        )
    }
    pub(super) fn record_scheduled_completion(
        &self,
        manager: &Manager,
        due: &DueTask,
        completion: Completion,
        now: u64,
    ) -> AppResult<()> {
        self.record_schedule_result_checked(
            &due.task,
            due.installation_generation,
            completion,
            now,
            &|| self.checked_scheduled_due(manager, due).map(|_| ()),
        )
    }
    fn record_schedule_result_checked(
        &self,
        task: &Task,
        installation_generation: u64,
        completion: Completion,
        now: u64,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<()> {
        let _operation = self.operation_guard()?;
        check()?;
        self.ensure_installation_current(&task.plugin_id)?;
        if !self.state.lock().unwrap().loaded.values().any(|l| {
            l.component.manifest().plugin_id == task.plugin_id
                && l.component.manifest().component_id == task.component_id
                && l.installation_generation == installation_generation
        }) {
            return Err(stale());
        }
        let mut registry = load(&self.root()?)?;
        let Some(stored) =
            registry
                .tasks
                .get_mut(&key(&task.plugin_id, &task.component_id, &task.task_id))
        else {
            return Err(stale());
        };
        if stored.action != task.action
            || stored.interval_seconds != task.interval_seconds
            || stored.revision != task.revision
        {
            return Err(stale());
        }
        if let Some(attempt) = stored.last_attempt.as_mut() {
            if attempt.state != Phase::Running
                || task
                    .last_attempt
                    .as_ref()
                    .is_none_or(|old| old.id != attempt.id)
            {
                return Err(stale());
            }
            attempt.state = match completion {
                Completion::Known(true) => Phase::Completed,
                Completion::Known(false) => Phase::Failed,
                Completion::Unconfirmed => Phase::Unconfirmed,
            };
            if matches!(completion, Completion::Known(_)) {
                attempt.completed_at_unix_ms = Some(now.to_string());
            }
            attempt.observed_at_unix_ms = now.to_string();
        }
        let delay_seconds = if matches!(completion, Completion::Known(true)) {
            stored.failure_count = 0;
            stored.interval_seconds
        } else if matches!(completion, Completion::Known(false)) {
            stored.failure_count = stored.failure_count.saturating_add(1).min(16);
            let exponent = u32::from(stored.failure_count.saturating_sub(1).min(4));
            60_u64
                .saturating_mul(1_u64 << exponent)
                .min(stored.interval_seconds)
        } else {
            stored.interval_seconds
        };
        stored.next_run_unix_ms = now.saturating_add(delay_seconds.saturating_mul(1000));
        save_checked(&self.root()?, &registry, check)
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn spawn(app: tauri::AppHandle) {
    start_worker(app, true);
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn resume(app: tauri::AppHandle) {
    start_worker(app, false);
}

fn start_worker(app: tauri::AppHandle, initial_delay: bool) {
    let owner = app
        .state::<crate::state::app_state::AppState>()
        .plugins()
        .scheduler
        .clone();
    if !owner.start() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if initial_delay {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        loop {
            let paused = {
                let state = app.state::<crate::state::app_state::AppState>();
                if state.is_shutting_down() || owner.is_closed() {
                    return;
                }
                state.plugins().platform.require_foreground().is_err()
                    || state.plugins().platform.execution_restriction().is_some()
            };
            if paused {
                owner.wake.notified().await;
                continue;
            }
            cycle(&app).await;
            tokio::select! {
                _ = owner.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_secs(30)) => {},
            }
        }
    });
}

async fn cycle(app: &tauri::AppHandle) {
    let cycle_app = app.clone();
    let due = tokio::task::spawn_blocking(move || {
        let state = cycle_app.state::<crate::state::app_state::AppState>();
        state
            .plugins()
            .take_due(state.capabilities(), crate::services::common::now_unix_ms())
    })
    .await
    .unwrap_or_else(|_| Err(AppError::internal("插件任务核对线程失败")));
    match due {
        Ok(tasks) => {
            for due in tasks {
                // Native suspension seals admission without waiting for IO/VM.
                // Dropping queued guards lets recovery distinguish never-dispatched work.
                if app
                    .state::<crate::state::app_state::AppState>()
                    .plugins()
                    .platform
                    .check_review_generation(due.review.host_generation.as_deref())
                    .is_err()
                {
                    break;
                }
                let task = due.task.clone();
                let task_app = app.clone();
                let task_plugin_id = task.plugin_id.clone();
                let task_id = task.task_id.clone();
                let execution_task = due.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let state = task_app.state::<crate::state::app_state::AppState>();
                    state
                        .plugins()
                        .invoke_scheduled(task_app.clone(), &execution_task)
                })
                .await
                .unwrap_or_else(|_| Err(AppError::internal("插件后台执行线程失败")));
                let state = app.state::<crate::state::app_state::AppState>();
                let declared_failure = result.as_ref().ok().is_some_and(|value| {
                    value.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
                });
                let succeeded = result.is_ok() && !declared_failure;
                let retry_error = AppError::invalid_argument("插件请求后台任务重试");
                let execution_result = if declared_failure {
                    Err(&retry_error)
                } else {
                    result.as_ref().map(|_| ())
                };
                crate::services::logs::plugin_host_event(
                    state.inner(),
                    &task.plugin_id,
                    &task.component_id,
                    crate::models::logs::PluginAction::ScheduledComponentAction,
                    execution_result,
                );
                if let Err(error) = state.plugins().record_scheduled_completion(
                    state.capabilities(),
                    &due,
                    if result.is_err() {
                        Completion::Unconfirmed
                    } else {
                        Completion::Known(succeeded)
                    },
                    crate::services::common::now_unix_ms(),
                ) {
                    crate::services::file_logger::line(&format!(
                        "plugin scheduler result checkpoint failed: {}",
                        error.message
                    ));
                    continue; // Late results do not become notices about a replacement intent.
                }
                if result.is_err() {
                    state.plugins().state.lock().unwrap().notices.push(format!(
                        "{}：任务 {} 结果未确认；先核对原任务，不自动重放",
                        task_plugin_id, task_id
                    ));
                } else if declared_failure {
                    crate::services::file_logger::line(&format!(
                        "plugin scheduled action requested retry: {task_plugin_id}/{task_id}"
                    ));
                }
            }
        }
        Err(error) => crate::services::file_logger::line(&format!(
            "plugin scheduler paused for this cycle: {}",
            error.message
        )),
    }
}

#[cfg(test)]
#[path = "scheduler/tests.rs"]
mod tests;

#[cfg(test)]
mod journal_tests;
