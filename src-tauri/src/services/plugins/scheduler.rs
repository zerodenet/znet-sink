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
}

#[derive(Clone)]
pub(super) struct DueTask {
    pub(super) task: Task,
    pub(super) installation_generation: u64,
    pub(super) review: Review,
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
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| AppError::internal("无法创建任务意图版本"))?;
        let task = Task {
            plugin_id: plugin_id.into(),
            component_id: component_id.into(),
            task_id,
            action,
            interval_seconds,
            next_run_unix_ms: crate::services::common::now_unix_ms()
                .saturating_add(interval_seconds.saturating_mul(1000)),
            failure_count: 0,
            revision: nonce.iter().map(|byte| format!("{byte:02x}")).collect(),
        };
        registry
            .tasks
            .insert(key(plugin_id, component_id, &task.task_id), task.clone());
        check()?;
        save(&self.root()?, &registry)?;
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
            check()?;
            save(&self.root()?, &registry)?;
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
        let mut due = Vec::new();
        for task in registry.tasks.values_mut() {
            if task.next_run_unix_ms <= now
                && enabled.contains(&(task.plugin_id.clone(), task.component_id.clone()))
            {
                let key = format!("{}/{}", task.plugin_id, task.component_id);
                let Some(policy) = policies.get(&key) else {
                    continue;
                };
                let installation_generation = *generations
                    .get(&(task.plugin_id.clone(), task.component_id.clone()))
                    .ok_or_else(stale)?;
                due.push(DueTask {
                    task: task.clone(),
                    installation_generation,
                    review: Review {
                        key,
                        identity: policy.identity.clone(),
                        registration: policy.registration,
                        revision: policy.revision,
                        host_generation: self.platform.review_generation(),
                    },
                });
                task.next_run_unix_ms =
                    now.saturating_add(task.interval_seconds.saturating_mul(1000));
            }
        }
        if !due.is_empty() {
            save(&self.root()?, &registry)?;
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
            })
        {
            return Err(stale());
        }
        Ok(binding)
    }

    #[cfg(test)]
    pub(super) fn record_schedule_result(
        &self,
        task: &Task,
        installation_generation: u64,
        succeeded: bool,
        now: u64,
    ) -> AppResult<()> {
        self.record_schedule_result_checked(task, installation_generation, succeeded, now, &|| {
            Ok(())
        })
    }
    fn record_scheduled_completion(
        &self,
        manager: &Manager,
        due: &DueTask,
        succeeded: bool,
        now: u64,
    ) -> AppResult<()> {
        self.record_schedule_result_checked(
            &due.task,
            due.installation_generation,
            succeeded,
            now,
            &|| self.checked_scheduled_due(manager, due).map(|_| ()),
        )
    }
    fn record_schedule_result_checked(
        &self,
        task: &Task,
        installation_generation: u64,
        succeeded: bool,
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
        let delay_seconds = if succeeded {
            stored.failure_count = 0;
            stored.interval_seconds
        } else {
            stored.failure_count = stored.failure_count.saturating_add(1).min(16);
            let exponent = u32::from(stored.failure_count.saturating_sub(1).min(4));
            60_u64
                .saturating_mul(1_u64 << exponent)
                .min(stored.interval_seconds)
        };
        stored.next_run_unix_ms = now.saturating_add(delay_seconds.saturating_mul(1000));
        check()?;
        save(&self.root()?, &registry)
    }
}

pub(crate) fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let due = {
                let state = app.state::<crate::state::app_state::AppState>();
                if state.is_shutting_down() {
                    return;
                }
                state
                    .plugins()
                    .take_due(state.capabilities(), crate::services::common::now_unix_ms())
            };
            match due {
                Ok(tasks) => {
                    for due in tasks {
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
                            succeeded,
                            crate::services::common::now_unix_ms(),
                        ) {
                            crate::services::file_logger::line(&format!(
                                "plugin scheduler result checkpoint failed: {}",
                                error.message
                            ));
                            continue; // Late results do not become notices about a replacement intent.
                        }
                        if let Err(error) = result {
                            state.plugins().state.lock().unwrap().notices.push(format!(
                                "{}：后台任务 {} 执行失败（{}）",
                                task_plugin_id, task_id, error.message
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
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
#[path = "scheduler/tests.rs"]
mod tests;
