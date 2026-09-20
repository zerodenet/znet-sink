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
        || registry.tasks.len() > 256
        || registry.tasks.iter().any(|(stored_key, task)| {
            stored_key != &key(&task.plugin_id, &task.component_id, &task.task_id)
                || !safe_id(&task.plugin_id)
                || !safe_id(&task.component_id)
                || !safe_id(&task.task_id)
                || !safe_id(&task.action)
                || task.failure_count > 16
                || !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&task.interval_seconds)
        })
    {
        return Err(AppError::invalid_argument("插件调度状态格式无效"));
    }
    Ok(registry)
}

fn save(root: &Path, registry: &Registry) -> AppResult<()> {
    let bytes = serde_json::to_vec(registry).map_err(io::failure)?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件调度状态超过容量限制"));
    }
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
    pub(super) fn schedule_put(
        &self,
        plugin_id: &str,
        component_id: &str,
        task_id: String,
        action: String,
        interval_seconds: u64,
    ) -> AppResult<Task> {
        if !safe_id(&task_id)
            || !safe_id(&action)
            || !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&interval_seconds)
        {
            return Err(AppError::invalid_argument(
                "插件后台任务名称、动作或间隔无效（间隔需为 5 分钟至 7 天）",
            ));
        }
        let _operation = self.operation.lock().unwrap();
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
        };
        registry
            .tasks
            .insert(key(plugin_id, component_id, &task.task_id), task.clone());
        save(&self.root()?, &registry)?;
        Ok(task)
    }
    pub(super) fn schedule_list(
        &self,
        plugin_id: &str,
        component_id: &str,
    ) -> AppResult<Vec<Task>> {
        let _operation = self.operation.lock().unwrap();
        Ok(load(&self.root()?)?
            .tasks
            .into_values()
            .filter(|task| task.plugin_id == plugin_id && task.component_id == component_id)
            .collect())
    }
    pub(super) fn schedule_delete(
        &self,
        plugin_id: &str,
        component_id: &str,
        task_id: &str,
    ) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let mut registry = load(&self.root()?)?;
        if registry
            .tasks
            .remove(&key(plugin_id, component_id, task_id))
            .is_some()
        {
            save(&self.root()?, &registry)?;
        }
        Ok(())
    }
    pub(super) fn remove_plugin_schedules(&self, plugin_id: &str) -> AppResult<()> {
        let mut registry = load(&self.root()?)?;
        let before = registry.tasks.len();
        registry.tasks.retain(|_, task| task.plugin_id != plugin_id);
        if registry.tasks.len() != before {
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
    fn take_due(&self, now: u64) -> AppResult<Vec<Task>> {
        let _operation = self.operation.lock().unwrap();
        let schedule_permission = Request {
            capability: Capability::TasksSchedule,
            scope: "self".into(),
        };
        let enabled: BTreeSet<_> = self
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
        let mut registry = load(&self.root()?)?;
        let mut due = Vec::new();
        for task in registry.tasks.values_mut() {
            if task.next_run_unix_ms <= now
                && enabled.contains(&(task.plugin_id.clone(), task.component_id.clone()))
            {
                due.push(task.clone());
                task.next_run_unix_ms =
                    now.saturating_add(task.interval_seconds.saturating_mul(1000));
            }
        }
        if !due.is_empty() {
            save(&self.root()?, &registry)?;
        }
        Ok(due)
    }

    fn record_schedule_result(&self, task: &Task, succeeded: bool, now: u64) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let mut registry = load(&self.root()?)?;
        let Some(stored) =
            registry
                .tasks
                .get_mut(&key(&task.plugin_id, &task.component_id, &task.task_id))
        else {
            return Ok(());
        };
        if stored.action != task.action || stored.interval_seconds != task.interval_seconds {
            return Ok(());
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
                    .take_due(crate::services::common::now_unix_ms())
            };
            match due {
                Ok(tasks) => {
                    for task in tasks {
                        let task_app = app.clone();
                        let task_plugin_id = task.plugin_id.clone();
                        let task_id = task.task_id.clone();
                        let execution_task = task.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let state = task_app.state::<crate::state::app_state::AppState>();
                            state.plugins().invoke_scheduled(
                                task_app.clone(),
                                &execution_task.plugin_id,
                                &execution_task.component_id,
                                format!("lifecycle.scheduled.{}", execution_task.action),
                                serde_json::json!({"taskId":execution_task.task_id}),
                            )
                        })
                        .await
                        .unwrap_or_else(|error| {
                            Err(AppError::internal(format!("插件后台执行线程失败：{error}")))
                        });
                        let state = app.state::<crate::state::app_state::AppState>();
                        let declared_failure = result.as_ref().ok().is_some_and(|value| {
                            value.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
                        });
                        let succeeded = result.is_ok() && !declared_failure;
                        if let Err(error) = state.plugins().record_schedule_result(
                            &task,
                            succeeded,
                            crate::services::common::now_unix_ms(),
                        ) {
                            crate::services::file_logger::line(&format!(
                                "plugin scheduler result checkpoint failed: {}",
                                error.message
                            ));
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
mod tests {
    use super::*;

    #[test]
    fn registry_round_trips_only_bounded_tasks() {
        let root = tempfile::tempdir().unwrap();
        let task = Task {
            plugin_id: "org.example.plugin".into(),
            component_id: "worker".into(),
            task_id: "refresh".into(),
            action: "refresh".into(),
            interval_seconds: MIN_INTERVAL_SECONDS,
            next_run_unix_ms: 100,
            failure_count: 0,
        };
        let mut registry = Registry {
            schema_version: schema(),
            ..Registry::default()
        };
        registry.tasks.insert(
            key(&task.plugin_id, &task.component_id, &task.task_id),
            task,
        );
        save(root.path(), &registry).unwrap();
        assert_eq!(load(root.path()).unwrap().tasks.len(), 1);
        assert!(!safe_id("../escape"));
    }

    #[test]
    fn failed_tasks_use_bounded_exponential_retry_and_success_resets_it() {
        let root = tempfile::tempdir().unwrap();
        let host = Host {
            root: Some(root.path().into()),
            ..Host::default()
        };
        let task = Task {
            plugin_id: "org.example.plugin".into(),
            component_id: "worker".into(),
            task_id: "refresh".into(),
            action: "refresh".into(),
            interval_seconds: 900,
            next_run_unix_ms: 100,
            failure_count: 0,
        };
        let mut registry = Registry {
            schema_version: schema(),
            ..Registry::default()
        };
        registry.tasks.insert(
            key(&task.plugin_id, &task.component_id, &task.task_id),
            task.clone(),
        );
        save(root.path(), &registry).unwrap();
        host.record_schedule_result(&task, false, 1_000).unwrap();
        let failed = load(root.path())
            .unwrap()
            .tasks
            .into_values()
            .next()
            .unwrap();
        assert_eq!(failed.failure_count, 1);
        assert_eq!(failed.next_run_unix_ms, 61_000);
        host.record_schedule_result(&failed, true, 2_000).unwrap();
        let recovered = load(root.path())
            .unwrap()
            .tasks
            .into_values()
            .next()
            .unwrap();
        assert_eq!(recovered.failure_count, 0);
        assert_eq!(recovered.next_run_unix_ms, 902_000);
    }
}
