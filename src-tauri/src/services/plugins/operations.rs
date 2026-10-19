use super::*;
use crate::services::common::now_unix_ms;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager as _;
use znet_plugin_sandbox::{
    contract::{Capability, LifecycleEvent},
    distribution::{package, MAX_PACKAGE_BYTES},
    runtime,
    sdk::{Call, Method},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvocationEnvelope {
    znet_plugin_result: u32,
    #[serde(default)]
    state_updates: BTreeMap<String, Option<String>>,
    value: serde_json::Value,
}

/// The guest cannot manufacture platform file selection authority by sending
/// an absolute path or a string pretending to be a host-issued ticket.
/// PageFrame's separate system-picker bridge retains its original lease path.
pub(super) fn validate_vm_sdk_admissibility(call: &Call) -> AppResult<()> {
    if matches!(call.method, Method::FileRead | Method::FileWrite) {
        return Err(AppError {
            code: "plugin_vm_file_unsupported",
            message: "插件 VM 文件读写尚未接入可信系统文件选择；不能使用脚本提供的路径".into(),
            details: None,
        });
    }
    Ok(())
}

fn sdk_dispatcher(
    app: tauri::AppHandle,
    plugin_id: &str,
    component_id: &str,
) -> runtime::HostSdkDispatcher {
    let plugin_id = plugin_id.to_owned();
    let component_id = component_id.to_owned();
    Arc::new(move |lease, input| {
        let mut observed_method = None;
        let result = (|| -> AppResult<serde_json::Value> {
            let call: Call = serde_json::from_str(input)
                .map_err(|_| AppError::invalid_argument("插件 SDK 请求格式无效"))?;
            observed_method = Some(call.method);
            validate_vm_sdk_admissibility(&call)?;
            let method = call.method;
            let request = znet_plugin_sandbox::contract::Request {
                capability: method.capability(),
                scope: call.request.scope.clone(),
            };
            let state = app.state::<crate::state::app_state::AppState>();
            if call.method == Method::SubscriptionApply {
                let (managed, request) = state
                    .plugins()
                    .managed_subscription_input_with_lease(&plugin_id, lease, call)?;
                return tauri::async_runtime::block_on(
                    crate::services::subscription::apply_managed_authorized(
                        app.clone(),
                        managed,
                        || {
                            lease
                                .check(Some(&request))
                                .map_err(super::sdk::invocation_failure)
                        },
                    ),
                )
                .and_then(|profile| serde_json::to_value(profile).map_err(io::failure));
            }
            if call.method == Method::SubscriptionMetadataUpdate {
                let (input, request) = state
                    .plugins()
                    .managed_subscription_metadata_with_lease(&plugin_id, lease, call)?;
                return crate::services::subscription::update_managed_metadata_authorized(
                    app.clone(),
                    input,
                    || {
                        lease
                            .check(Some(&request))
                            .map_err(super::sdk::invocation_failure)
                    },
                )
                .and_then(|profile| serde_json::to_value(profile).map_err(io::failure));
            }
            if call.method == Method::SubscriptionSyncComplete {
                let (input, request) = state
                    .plugins()
                    .managed_subscription_sync_complete_with_lease(&plugin_id, lease, call)?;
                return crate::services::subscription::complete_managed_sync_authorized(
                    app.clone(),
                    input,
                    || {
                        lease
                            .check(Some(&request))
                            .map_err(super::sdk::invocation_failure)
                    },
                )
                .and_then(|profile| serde_json::to_value(profile).map_err(io::failure));
            }
            if call.method == Method::SubscriptionRemove {
                let (subscription_id, remove_associated_config, request) = state
                    .plugins()
                    .managed_subscription_removal_with_lease(lease, call)?;
                return tauri::async_runtime::block_on(
                    crate::services::subscription::remove_managed_authorized(
                        app.clone(),
                        subscription_id,
                        remove_associated_config,
                        plugin_id.clone(),
                        || {
                            lease
                                .check(Some(&request))
                                .map_err(super::sdk::invocation_failure)
                        },
                    ),
                )
                .and_then(|outcome| serde_json::to_value(outcome).map_err(io::failure));
            }
            let network_proxy = crate::configuration::preferences::endpoint(state.inner())
                .ok()
                .map(|(host, port)| format!("http://{host}:{port}"));
            let value = state.plugins().sdk_call_with_lease(
                state.capabilities(),
                &plugin_id,
                &component_id,
                lease,
                call,
                network_proxy,
                Some(state.inner()),
            )?;
            let value = if method == Method::BrowserOpen {
                super::sdk::open_browser_with_lease(&app, lease, &request, value)?
            } else {
                value
            };
            super::notification::deliver_if_post(method, value, |value| {
                super::notification::publish(&app, value)
            })
        })();
        if result.is_err()
            || matches!(
                observed_method,
                Some(
                    Method::SubscriptionApply
                        | Method::SubscriptionMetadataUpdate
                        | Method::SubscriptionSyncComplete
                        | Method::SubscriptionRemove
                        | Method::ProtectedLoad
                )
            )
        {
            let state = app.state::<crate::state::app_state::AppState>();
            let action = observed_method.map_or(
                crate::models::logs::PluginAction::ComponentAction,
                crate::services::logs::plugin_sdk_action,
            );
            crate::services::logs::plugin_host_event(
                state.inner(),
                &plugin_id,
                &component_id,
                action,
                result.as_ref().map(|_| ()),
            );
        }
        let reply = match result {
            Ok(value) => serde_json::json!({"version":1,"ok":true,"value":value}),
            Err(error) => serde_json::to_value(znet_plugin_sandbox::sdk::Reply {
                version: znet_plugin_sandbox::sdk::SDK_VERSION,
                ok: false,
                value: None,
                error: Some(super::sdk::public_failure(error)),
            })
            .map_err(|_| znet_plugin_sandbox::contract::Error::InvalidOutput)?,
        };
        serde_json::to_string(&reply)
            .map_err(|_| znet_plugin_sandbox::contract::Error::InvalidOutput)
    })
}

impl Host {
    /// Clone this owner's handle before AppState moves into the application.
    /// Closing it does not close other hosts sharing the global VM budget.
    pub(crate) fn executor_shutdown_handle(&self) -> runtime::Executor {
        self.executor
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn refresh(&self, manager: &Manager) -> AppResult<Snapshot> {
        let _operation = self.operation_guard()?;
        let directory = match io::directory(manager) {
            Ok(directory) => directory,
            Err(error) => return Err(error),
        };
        local_state::save_directory(&self.root()?, &directory)?;
        self.state.lock().unwrap().directory =
            local_state::effective_directory(&self.root()?, Some(directory))?;
        self.checked_rescan(manager)?;
        Ok(self.snapshot(manager))
    }
    pub fn authorize(
        &self,
        manager: &Manager,
        review: Review,
        grants: Vec<Request>,
    ) -> AppResult<Snapshot> {
        let _operation = self.operation_guard()?;
        self.checked_rescan(manager)?;
        if grants.iter().any(|request| !self.supported(request)) {
            return Err(AppError::invalid_argument("所选权限尚未开放"));
        }
        let current = self.current_review(manager, &review)?;
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&review.key).ok_or_else(stale)?;
        if loaded.blocked.is_some() {
            return Err(stale());
        }
        if loaded
            .component
            .manifest()
            .configuration
            .as_ref()
            .is_some_and(|schema| {
                let mut values = schema.defaults();
                values.extend(loaded.configuration.clone());
                schema.validate_values(&values).is_err()
            })
        {
            return Err(AppError::invalid_argument("请先完成插件配置再启用"));
        }
        if loaded
            .component
            .manifest()
            .required
            .iter()
            .any(|r| !grants.contains(r))
        {
            return Err(AppError::invalid_argument(
                "请允许必需权限，或取消启用此组件",
            ));
        }
        let publisher_fingerprint = loaded.publisher_fingerprint.clone();
        let manifest = loaded.component.manifest();
        let plugin_id = manifest.plugin_id.clone();
        let component_id = manifest.component_id.clone();
        let previous_grants = loaded.consent.grants.clone();
        let declaration = manifest
            .required
            .iter()
            .chain(&manifest.optional)
            .cloned()
            .collect();
        let selected = grants.iter().cloned().collect();
        drop(state);
        manager
            .authorize_component_persistent(&current, grants.iter().map(permission).collect())
            .map_err(|error| {
                if error == znet_client_core::capability::Error::Revoked {
                    stale()
                } else {
                    io::failure(error)
                }
            })?;
        if let Err(error) = local_state::approve(
            &self.root()?,
            &publisher_fingerprint,
            &plugin_id,
            &component_id,
            selected,
            declaration,
        ) {
            manager.revoke_component(&current.key);
            return Err(error);
        }
        self.rescan(manager, &self.store()?)?;
        let removed_capability = |capability| {
            previous_grants
                .iter()
                .any(|request| request.capability == capability)
                && !grants
                    .iter()
                    .any(|request| request.capability == capability)
        };
        let owner = format!("{plugin_id}/{component_id}");
        if removed_capability(Capability::TasksSchedule) {
            self.remove_component_schedules(&plugin_id, &component_id)?;
        }
        if removed_capability(Capability::BrowserCallback) {
            self.callbacks.cancel_owner(&owner);
        }
        if [
            Capability::MaterialsSubmit,
            Capability::SecretsSessionReceive,
            Capability::CryptoSessionUse,
            Capability::RuntimeProtectedLoad,
        ]
        .into_iter()
        .any(removed_capability)
        {
            self.sensitive.clear_owner(&owner);
        }
        Ok(self.snapshot(manager))
    }
    pub fn stop(&self, manager: &Manager, key: &str) -> AppResult<Snapshot> {
        self.stop_checked(manager, key, None)
    }
    pub fn stop_reviewed(&self, manager: &Manager, review: &Review) -> AppResult<Snapshot> {
        self.stop_checked(manager, &review.key, Some(review))
    }
    fn stop_checked(
        &self,
        manager: &Manager,
        key: &str,
        review: Option<&Review>,
    ) -> AppResult<Snapshot> {
        let _operation = self.operation_guard()?;
        self.checked_rescan(manager)?;
        if let Some(review) = review {
            self.current_review(manager, review)?;
        }
        let (publisher, plugin_id, component_id) = {
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(key).ok_or_else(stale)?;
            (
                loaded.publisher_fingerprint.clone(),
                loaded.component.manifest().plugin_id.clone(),
                loaded.component.manifest().component_id.clone(),
            )
        };
        local_state::set_enabled(&self.root()?, &publisher, &plugin_id, &component_id, false)?;
        manager.revoke_component(key);
        self.remove_component_schedules(&plugin_id, &component_id)?;
        self.callbacks.cancel_owner(key);
        self.sensitive.clear_owner(key);
        if let Some(loaded) = self.state.lock().unwrap().loaded.get_mut(key) {
            loaded.consent.enabled = false;
        }
        Ok(self.snapshot(manager))
    }
    pub fn revoke_permissions(&self, manager: &Manager, key: &str) -> AppResult<Snapshot> {
        self.revoke_permissions_checked(manager, key, None)
    }
    pub fn revoke_permissions_reviewed(
        &self,
        manager: &Manager,
        review: &Review,
    ) -> AppResult<Snapshot> {
        self.revoke_permissions_checked(manager, &review.key, Some(review))
    }
    fn revoke_permissions_checked(
        &self,
        manager: &Manager,
        key: &str,
        review: Option<&Review>,
    ) -> AppResult<Snapshot> {
        let _operation = self.operation_guard()?;
        self.checked_rescan(manager)?;
        if let Some(review) = review {
            self.current_review(manager, review)?;
        }
        let (publisher, plugin_id, component_id) = {
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(key).ok_or_else(stale)?;
            (
                loaded.publisher_fingerprint.clone(),
                loaded.component.manifest().plugin_id.clone(),
                loaded.component.manifest().component_id.clone(),
            )
        };
        local_state::revoke(&self.root()?, &publisher, &plugin_id, &component_id)?;
        manager.revoke_component(key);
        self.remove_component_schedules(&plugin_id, &component_id)?;
        self.callbacks.cancel_owner(key);
        self.sensitive.clear_owner(key);
        if let Some(loaded) = self.state.lock().unwrap().loaded.get_mut(key) {
            loaded.consent = local_state::ComponentConsent::default();
        }
        Ok(self.snapshot(manager))
    }
    pub fn run(&self, manager: &Manager, review: Review) -> AppResult<serde_json::Value> {
        self.run_with_host(manager, review, None)
    }

    pub fn run_with_host(
        &self,
        manager: &Manager,
        review: Review,
        app: Option<tauri::AppHandle>,
    ) -> AppResult<serde_json::Value> {
        let plugin = review.key.split_once('/').ok_or_else(stale)?.0;
        let _activity = self.activity_checked(manager, plugin)?;

        let (component, authority, configuration, executor, target) = {
            let _operation = self.operation_guard()?;
            self.checked_rescan(manager)?;
            self.current_review(manager, &review).map_err(|_| {
                super::sdk::invocation_failure(znet_plugin_sandbox::contract::Error::Revoked)
            })?;
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(&review.key).ok_or_else(stale)?;
            let mut configuration = loaded
                .component
                .manifest()
                .configuration
                .as_ref()
                .map_or_else(BTreeMap::new, |schema| schema.defaults());
            configuration.extend(loaded.configuration.clone());
            (
                loaded.component.clone(),
                loaded.authority.clone().ok_or_else(stale)?,
                configuration,
                self.dispatch_executor()?,
                self.platform.target()?,
            )
        };
        let input = serde_json::json!({ "configuration": configuration });
        let result = executor
            .execute_reviewed(
                &component,
                &authority,
                input,
                runtime::ReviewedExecution {
                    target,
                    registration: review.registration,
                    revision: review.revision,
                    cancelled: Arc::new(AtomicBool::new(false)),
                    host_version: env!("CARGO_PKG_VERSION").into(),
                    dispatcher: app.map(|app| {
                        sdk_dispatcher(
                            app,
                            &component.manifest().plugin_id,
                            &component.manifest().component_id,
                        )
                    }),
                    scheduled: false,
                },
            )
            .and_then(|receipt| receipt.into_value());
        result.map_err(super::sdk::invocation_failure)
    }
    pub fn invoke(
        &self,
        manager: &Manager,
        app: Option<tauri::AppHandle>,
        plugin_id: &str,
        component_id: &str,
        action: String,
        payload: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        self.invoke_inner(
            manager,
            app,
            plugin_id,
            component_id,
            action,
            payload,
            None,
            Arc::new(AtomicBool::new(false)),
            None,
        )
    }
    pub fn invoke_reviewed(
        &self,
        manager: &Manager,
        app: Option<tauri::AppHandle>,
        review: Review,
        action: String,
        payload: serde_json::Value,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<serde_json::Value> {
        let (plugin_id, component_id) = review.key.split_once('/').ok_or_else(stale)?;
        self.invoke_inner(
            manager,
            app,
            plugin_id,
            component_id,
            action,
            payload,
            Some(&review),
            cancelled,
            None,
        )
    }
    /// Test-only synchronization at a real VM/native SDK callback. The same
    /// production owning lock, original lease and publication path are used.
    #[cfg(test)]
    pub(super) fn invoke_reviewed_with_test_dispatcher(
        &self,
        manager: &Manager,
        review: Review,
        cancelled: Arc<AtomicBool>,
        dispatcher: runtime::HostSdkDispatcher,
    ) -> AppResult<serde_json::Value> {
        let (plugin_id, component_id) = review.key.split_once('/').ok_or_else(stale)?;
        self.invoke_inner(
            manager,
            None,
            plugin_id,
            component_id,
            "read".into(),
            serde_json::Value::Null,
            Some(&review),
            cancelled,
            Some(dispatcher),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke_inner(
        &self,
        manager: &Manager,
        app: Option<tauri::AppHandle>,
        plugin_id: &str,
        component_id: &str,
        action: String,
        payload: serde_json::Value,
        original_review: Option<&Review>,
        cancelled: Arc<AtomicBool>,
        injected_dispatcher: Option<runtime::HostSdkDispatcher>,
    ) -> AppResult<serde_json::Value> {
        let _activity = self.activity_checked(manager, plugin_id)?;

        if action.is_empty()
            || action.len() > 128
            || !action
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || serde_json::to_vec(&payload).map_err(io::failure)?.len() > 16 * 1024
        {
            return Err(AppError::invalid_argument("插件页面操作参数无效"));
        }
        let (
            component,
            authority,
            configuration,
            publisher,
            plugin_state,
            binding,
            executor,
            target,
            publication_review,
        ) = {
            let _operation = self.operation_guard()?;
            self.checked_rescan(manager)?;
            if cancelled.load(Ordering::Acquire) {
                return Err(super::sdk::invocation_failure(
                    znet_plugin_sandbox::contract::Error::Cancelled,
                ));
            }
            let key = format!("{plugin_id}/{component_id}");
            let binding = if let Some(review) = original_review {
                self.current_review(manager, review).map_err(|_| {
                    super::sdk::invocation_failure(znet_plugin_sandbox::contract::Error::Revoked)
                })?
            } else {
                manager
                    .component_snapshots()
                    .into_iter()
                    .find(|entry| entry.key == key && !entry.retired)
                    .ok_or_else(stale)?
            };
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(&key).ok_or_else(stale)?;
            if loaded.component.manifest().plugin_id != plugin_id
                || loaded.component.manifest().component_id != component_id
            {
                return Err(stale());
            }
            let mut configuration = loaded
                .component
                .manifest()
                .configuration
                .as_ref()
                .map_or_else(BTreeMap::new, |schema| schema.defaults());
            configuration.extend(loaded.configuration.clone());
            let publisher = loaded.publisher_fingerprint.clone();
            let plugin_state = namespace::runtime_state(&self.root()?, &publisher, plugin_id)?;
            let publication_review = Review {
                key: binding.key.clone(),
                identity: binding.identity.clone(),
                registration: binding.registration,
                revision: binding.revision,
                host_generation: self.platform.review_generation(),
            };
            (
                loaded.component.clone(),
                loaded.authority.clone().ok_or_else(stale)?,
                configuration,
                publisher,
                plugin_state,
                binding,
                self.dispatch_executor()?,
                self.platform.target()?,
                publication_review,
            )
        };
        let digest = component.digest().to_owned();
        let input = serde_json::json!({
            "configuration": configuration,
            "state": plugin_state,
            "invocation": { "action": action, "payload": payload }
        });
        let receipt = executor
            .execute_reviewed(
                &component,
                &authority,
                input,
                runtime::ReviewedExecution {
                    target,
                    registration: binding.registration,
                    revision: binding.revision,
                    cancelled,
                    host_version: env!("CARGO_PKG_VERSION").into(),
                    dispatcher: app
                        .map(|app| sdk_dispatcher(app, plugin_id, component_id))
                        .or(injected_dispatcher),
                    scheduled: false,
                },
            )
            .map_err(super::sdk::invocation_failure)?;
        let output = receipt
            .value()
            .map_err(super::sdk::invocation_failure)?
            .clone();
        let Some(marker) = output
            .as_object()
            .and_then(|object| object.get("znet_plugin_result"))
        else {
            receipt.check().map_err(super::sdk::invocation_failure)?;
            return Ok(output);
        };
        if marker != 1 {
            return Err(AppError::invalid_argument("插件返回了不支持的结果协议"));
        }
        let envelope: InvocationEnvelope = serde_json::from_value(output)
            .map_err(|_| AppError::invalid_argument("插件返回的状态更新格式无效"))?;
        if envelope.znet_plugin_result != 1 {
            return Err(AppError::invalid_argument("插件返回了不支持的结果协议"));
        }
        let _operation = self.operation_guard()?;
        receipt.check().map_err(super::sdk::invocation_failure)?;
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        if loaded.component.digest() != digest || !loaded.consent.enabled {
            return Err(stale());
        }
        drop(state);
        namespace::apply_runtime_updates_checked(
            &self.root()?,
            &publisher,
            plugin_id,
            envelope.state_updates,
            &|| {
                receipt.check().map_err(super::sdk::invocation_failure)?;
                self.current_review(manager, &publication_review)?;
                Ok(())
            },
        )?;
        receipt.check().map_err(super::sdk::invocation_failure)?;
        Ok(envelope.value)
    }

    /// Execute a declared scheduled action with the same typed SDK surface as
    /// signed management pages. The scheduler and host stay business-neutral;
    /// the plugin owns the action and protocol.
    pub(super) fn invoke_scheduled(
        &self,
        app: tauri::AppHandle,
        due: &super::scheduler::DueTask,
    ) -> AppResult<serde_json::Value> {
        let plugin_id = due.task.plugin_id.as_str();
        let component_id = due.task.component_id.as_str();
        let _activity = self.activity_checked(
            app.state::<crate::state::app_state::AppState>()
                .capabilities(),
            plugin_id,
        )?;

        let (
            component,
            authority,
            configuration,
            publisher,
            plugin_state,
            binding,
            executor,
            target,
        ) = {
            let _operation = self.operation_guard()?;
            let manager = app.state::<crate::state::app_state::AppState>();
            self.checked_rescan(manager.capabilities())?;
            let key = format!("{plugin_id}/{component_id}");
            let binding = self.checked_scheduled_due(manager.capabilities(), due)?;
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(&key).ok_or_else(stale)?;
            let mut configuration = loaded
                .component
                .manifest()
                .configuration
                .as_ref()
                .map_or_else(BTreeMap::new, |schema| schema.defaults());
            configuration.extend(loaded.configuration.clone());
            (
                loaded.component.clone(),
                loaded.authority.clone().ok_or_else(stale)?,
                configuration,
                loaded.publisher_fingerprint.clone(),
                namespace::runtime_state(&self.root()?, &loaded.publisher_fingerprint, plugin_id)?,
                binding,
                self.dispatch_executor()?,
                self.platform.target()?,
            )
        };
        let digest = component.digest().to_owned();
        let dispatcher = sdk_dispatcher(app.clone(), plugin_id, component_id);
        let receipt = executor
            .execute_reviewed(
                &component,
                &authority,
                serde_json::json!({
                    "configuration": configuration,
                    "state": plugin_state,
                    "invocation": {
                        "action": format!("lifecycle.scheduled.{}", due.task.action),
                        "payload": {"taskId": due.task.task_id},
                        "now_unix_ms": now_unix_ms()
                    }
                }),
                runtime::ReviewedExecution {
                    target,
                    registration: binding.registration,
                    revision: binding.revision,
                    cancelled: Arc::new(AtomicBool::new(false)),
                    host_version: env!("CARGO_PKG_VERSION").into(),
                    dispatcher: Some(dispatcher),
                    scheduled: true,
                },
            )
            .map_err(super::sdk::invocation_failure)?;
        let output = receipt
            .value()
            .map_err(super::sdk::invocation_failure)?
            .clone();
        let envelope: InvocationEnvelope = serde_json::from_value(output)
            .map_err(|_| AppError::invalid_argument("插件返回的状态更新格式无效"))?;
        if envelope.znet_plugin_result != 1 {
            return Err(AppError::invalid_argument("插件返回了不支持的结果协议"));
        }
        let _operation = self.operation_guard()?;
        receipt.check().map_err(super::sdk::invocation_failure)?;
        self.checked_scheduled_due(
            app.state::<crate::state::app_state::AppState>()
                .capabilities(),
            due,
        )?;
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        if loaded.component.digest() != digest || !loaded.consent.enabled {
            return Err(stale());
        }
        drop(state);
        namespace::apply_runtime_updates_checked(
            &self.root()?,
            &publisher,
            plugin_id,
            envelope.state_updates,
            &|| {
                receipt.check().map_err(super::sdk::invocation_failure)?;
                self.checked_scheduled_due(
                    app.state::<crate::state::app_state::AppState>()
                        .capabilities(),
                    due,
                )?;
                Ok(())
            },
        )?;
        receipt.check().map_err(super::sdk::invocation_failure)?;
        Ok(envelope.value)
    }
    /// Run declared bounded startup hooks after the app state is available.
    /// A hook receives its plugin's small opaque state snapshot and may return
    /// an atomic state-update envelope. Failures stay isolated to that plugin.
    #[cfg(test)]
    pub fn start_enabled(&self, manager: &Manager) {
        self.start_enabled_with_app(manager, None);
    }
    pub fn start_enabled_with_app(&self, manager: &Manager, app: Option<tauri::AppHandle>) {
        if self.platform.require_foreground().is_err()
            || self.platform.execution_restriction().is_some()
            || self.startup_dispatched.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let snapshot = self.snapshot(manager);
        let lifecycle: Vec<_> = {
            let state = self.state.lock().unwrap();
            state
                .loaded
                .values()
                .filter(|loaded| {
                    loaded
                        .component
                        .manifest()
                        .lifecycle
                        .contains(&LifecycleEvent::HostStart)
                })
                .filter(|loaded| {
                    snapshot.components.iter().any(|component| {
                        component.plugin_id == loaded.component.manifest().plugin_id
                            && component.component_id == loaded.component.manifest().component_id
                            && component.enabled
                    })
                })
                .filter_map(|loaded| {
                    snapshot
                        .components
                        .iter()
                        .find(|component| {
                            component.plugin_id == loaded.component.manifest().plugin_id
                                && component.component_id
                                    == loaded.component.manifest().component_id
                        })
                        .and_then(|component| component.review.clone())
                })
                .collect()
        };
        let mut notices = Vec::new();
        for review in lifecycle {
            if let Err(error) = self.invoke_reviewed(
                manager,
                app.clone(),
                review.clone(),
                "lifecycle.host_start".into(),
                serde_json::Value::Null,
                Arc::new(AtomicBool::new(false)),
            ) {
                notices.push(format!("{}：启动检查失败（{}）", review.key, error.message));
            }
        }
        self.state.lock().unwrap().notices.extend(notices);
    }
    pub fn configure(
        &self,
        manager: &Manager,
        key: &str,
        values: BTreeMap<String, String>,
    ) -> AppResult<Snapshot> {
        self.configure_inner(manager, key, values, None, Arc::new(AtomicBool::new(false)))
    }
    pub fn configure_reviewed(
        &self,
        manager: &Manager,
        review: Review,
        values: BTreeMap<String, String>,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<Snapshot> {
        self.configure_inner(manager, &review.key, values, Some(&review), cancelled)
    }
    fn configure_inner(
        &self,
        manager: &Manager,
        key: &str,
        values: BTreeMap<String, String>,
        original_review: Option<&Review>,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<Snapshot> {
        let _operation = self.operation_guard()?;
        self.checked_rescan(manager)?;
        if cancelled.load(Ordering::Acquire) {
            return Err(super::sdk::invocation_failure(
                znet_plugin_sandbox::contract::Error::Cancelled,
            ));
        }
        if let Some(review) = original_review {
            self.current_review(manager, review).map_err(|_| {
                super::sdk::invocation_failure(znet_plugin_sandbox::contract::Error::Revoked)
            })?;
        }
        let schema = {
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(key).ok_or_else(stale)?;
            loaded
                .component
                .manifest()
                .configuration
                .clone()
                .ok_or_else(|| AppError::invalid_argument("此组件未声明配置界面"))?
        };
        schema
            .validate_values(&values)
            .map_err(|_| AppError::invalid_argument("插件配置缺少必填项或字段格式无效"))?;
        if cancelled.load(Ordering::Acquire) {
            return Err(super::sdk::invocation_failure(
                znet_plugin_sandbox::contract::Error::Cancelled,
            ));
        }
        manager.revoke_component(key);
        configuration::save(&self.root()?, key, values)?;
        self.rescan(manager, &self.store()?)?;
        if cancelled.load(Ordering::Acquire) {
            return Err(super::sdk::invocation_failure(
                znet_plugin_sandbox::contract::Error::Cancelled,
            ));
        }
        Ok(self.snapshot(manager))
    }
    #[cfg(test)]
    pub fn uninstall(
        &self,
        _manager: &Manager,
        _id: &str,
        _cleanup: impl FnOnce() -> AppResult<()>,
    ) -> AppResult<Snapshot> {
        Err(AppError {
            code: "plugin_uninstall_preview_required",
            message: "卸载必须在主应用预览固定资源并明确确认".into(),
            details: None,
        })
    }

    fn selected_package(&self, manager: &Manager, path: String) -> AppResult<Vec<u8>> {
        let selected = znet_client_capabilities::files::Selection::from_user_path(path.into());
        let lease = io::lease(manager, BTreeSet::from([selected.permission()]))?;
        znet_client_capabilities::files::read(&lease, &selected, MAX_PACKAGE_BYTES)
            .and_then(|resource| resource.take(&lease))
            .map_err(io::failure)
    }
    pub fn preview_install(&self, manager: &Manager, path: String) -> AppResult<InstallReview> {
        self.platform.require_installation()?;
        let _operation = self.operation_guard()?;
        let bytes = self.selected_package(manager, path)?;
        let plugin_id = package::package_id(&bytes).map_err(io::failure)?;
        self.preview_local_bytes(manager, &bytes, &plugin_id)
    }
    pub fn install(
        &self,
        manager: &Manager,
        path: String,
        approval_digest: Option<String>,
    ) -> AppResult<Snapshot> {
        self.platform.require_installation()?;
        let _operation = self.operation_guard()?;
        let bytes = self.selected_package(manager, path)?;
        let plugin_id = package::package_id(&bytes).map_err(io::failure)?;
        self.install_local_bytes(manager, &bytes, &plugin_id, approval_digest.as_deref())
    }

    fn local_candidate(
        &self,
        manager: &Manager,
        bytes: &[u8],
        id: &str,
    ) -> AppResult<(package::VerifiedPackage, Registration)> {
        let base = match package::embedded_registration(bytes).map_err(io::failure)? {
            Some(registration) => registration,
            None => {
                // Early JSON packages without an embedded key can migrate through
                // a cached/public publisher key. New local packages embed identity.
                let central = match local_state::load_directory(&self.root()?)? {
                    Some(cached) => cached,
                    None => io::directory(manager)?,
                };
                central.find(id).map_err(|_| {
                    AppError::invalid_argument(
                        "此插件包未携带发布者公钥，且插件中心没有可用于验签的登记；请发布者重新生成携带发布者登记的应用包",
                    )
                })?.clone()
            }
        };
        let verified = package::verify_local(bytes, &base).map_err(io::failure)?;
        let mut registration = base;
        registration.releases.clear();
        registration.capabilities.clear();
        for component in &verified.components {
            for request in component
                .manifest()
                .required
                .iter()
                .chain(&component.manifest().optional)
            {
                let capability = serde_json::to_value(request.capability)
                    .map_err(io::failure)?
                    .as_str()
                    .ok_or_else(|| AppError::invalid_argument("插件权限声明格式无效"))?
                    .to_owned();
                if !registration.capabilities.contains(&capability) {
                    registration.capabilities.push(capability);
                }
            }
        }
        registration.surfaces = if verified.pages.is_empty() {
            Vec::new()
        } else {
            vec!["znet-sink.ui.management.v1".into()]
        };

        self.restore_cached(manager);
        if let Some(current) = self
            .state
            .lock()
            .unwrap()
            .directory
            .as_ref()
            .and_then(|directory| directory.find(id).ok())
            .cloned()
        {
            if current.publisher.public_key != registration.publisher.public_key {
                return Err(AppError::invalid_argument(
                    "发布者签名与已安装插件不一致；如需更换发布者，请先卸载当前插件",
                ));
            }
            // Retain prior declarations so the package store can verify its
            // rollback copy. Runtime authority still comes only from each
            // component's manifest and the user's component consent.
            for capability in current.capabilities {
                if !registration.capabilities.contains(&capability) {
                    registration.capabilities.push(capability);
                }
            }
            for surface in current.surfaces {
                if !registration.surfaces.contains(&surface) {
                    registration.surfaces.push(surface);
                }
            }
        }
        registration.validate().map_err(io::failure)?;
        Ok((verified, registration))
    }

    pub(super) fn preview_local_bytes(
        &self,
        manager: &Manager,
        bytes: &[u8],
        id: &str,
    ) -> AppResult<InstallReview> {
        let (verified, registration) = self.local_candidate(manager, bytes, id)?;
        let current_registration = self
            .state
            .lock()
            .unwrap()
            .directory
            .as_ref()
            .and_then(|directory| directory.find(id).ok())
            .cloned();
        self.review_verified(verified, &registration, current_registration.as_ref(), true)
    }
    pub(super) fn preview_bytes(
        &self,
        bytes: &[u8],
        directory: &Directory,
        id: &str,
    ) -> AppResult<InstallReview> {
        let registration = directory.find(id).map_err(io::failure)?;
        let verified = package::verify(bytes, registration).map_err(io::failure)?;
        self.review_verified(verified, registration, Some(registration), false)
    }

    fn review_verified(
        &self,
        verified: package::VerifiedPackage,
        registration: &Registration,
        current_registration: Option<&Registration>,
        local_trust: bool,
    ) -> AppResult<InstallReview> {
        let requested_surfaces = if verified.pages.is_empty() {
            Vec::new()
        } else {
            vec!["znet-sink.ui.management.v1".into()]
        };
        let target = self.platform.target()?;
        verified
            .compatible(&target, env!("CARGO_PKG_VERSION"))
            .map_err(io::failure)?;
        let store = self.store()?;
        let current = if store.list().map_err(io::failure)?.contains(&verified.id) {
            let trusted = current_registration.unwrap_or(registration);
            Some(store.current(trusted).map_err(io::failure)?)
        } else {
            None
        };
        if current.is_some() {
            ensure_newer(
                &store,
                current_registration.unwrap_or(registration),
                &verified.version,
            )?;
        }
        let mut added_permissions = Vec::new();
        let mut removed_permissions = Vec::new();
        if let Some(current) = &current {
            let current_components: BTreeMap<_, _> = current
                .components
                .iter()
                .map(|component| {
                    (
                        component.manifest().component_id.as_str(),
                        component.manifest(),
                    )
                })
                .collect();
            let candidate_components: BTreeMap<_, _> = verified
                .components
                .iter()
                .map(|component| {
                    (
                        component.manifest().component_id.as_str(),
                        component.manifest(),
                    )
                })
                .collect();
            for component in &verified.components {
                let manifest = component.manifest();
                let old: BTreeSet<_> = current_components
                    .get(manifest.component_id.as_str())
                    .map(|old| old.required.iter().chain(&old.optional).cloned().collect())
                    .unwrap_or_default();
                for request in manifest.required.iter().chain(&manifest.optional) {
                    if !old.contains(request) {
                        added_permissions.push(PermissionChangeView {
                            component_id: manifest.component_id.clone(),
                            request: request.clone(),
                            required: manifest.required.contains(request),
                        });
                    }
                }
            }
            for component in &current.components {
                let manifest = component.manifest();
                let candidate: BTreeSet<_> = candidate_components
                    .get(manifest.component_id.as_str())
                    .map(|next| {
                        next.required
                            .iter()
                            .chain(&next.optional)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                for request in manifest.required.iter().chain(&manifest.optional) {
                    if !candidate.contains(request) {
                        removed_permissions.push(PermissionChangeView {
                            component_id: manifest.component_id.clone(),
                            request: request.clone(),
                            required: manifest.required.contains(request),
                        });
                    }
                }
            }
        }
        let first_install = current.is_none();
        let surface_expanded = current
            .as_ref()
            .is_some_and(|package| package.pages.is_empty() && !requested_surfaces.is_empty());
        Ok(InstallReview {
            host_generation: self.platform.review_generation(),
            plugin_id: verified.id,
            current_version: current.map(|package| package.version),
            candidate_version: verified.version,
            candidate_digest: verified.digest,
            publisher: registration.publisher.id.clone(),
            publisher_fingerprint: publisher_fingerprint(registration)?,
            first_install,
            local_trust,
            requested_surfaces,
            requires_approval: !added_permissions.is_empty()
                || surface_expanded
                || (local_trust && first_install),
            added_permissions,
            removed_permissions,
        })
    }

    pub(super) fn install_local_bytes(
        &self,
        manager: &Manager,
        bytes: &[u8],
        id: &str,
        approval_digest: Option<&str>,
    ) -> AppResult<Snapshot> {
        self.platform.require_installation()?;
        let (verified, registration) = self.local_candidate(manager, bytes, id)?;
        let current_registration = self
            .state
            .lock()
            .unwrap()
            .directory
            .as_ref()
            .and_then(|directory| directory.find(id).ok())
            .cloned();
        let review =
            self.review_verified(verified, &registration, current_registration.as_ref(), true)?;
        if (review.requires_approval || approval_digest.is_some())
            && approval_digest != Some(review.candidate_digest.as_str())
        {
            return Err(AppError::invalid_argument(
                "本地安装需要先确认发布者指纹、权限和管理页面声明",
            ));
        }
        self.install_verified(manager, bytes, registration, None)
    }
    pub(super) fn install_bytes(
        &self,
        manager: &Manager,
        bytes: &[u8],
        directory: Directory,
        id: &str,
        approval_digest: Option<&str>,
    ) -> AppResult<Snapshot> {
        self.platform.require_installation()?;
        let review = self.preview_bytes(bytes, &directory, id)?;
        if (review.requires_approval || approval_digest.is_some())
            && approval_digest != Some(review.candidate_digest.as_str())
        {
            return Err(AppError::invalid_argument(
                "新版本申请了新增或扩大权限，请先查看权限变化并明确确认",
            ));
        }
        let registration = directory.find(id).map_err(io::failure)?;
        self.install_verified(manager, bytes, registration.clone(), Some(directory))
    }

    fn install_verified(
        &self,
        manager: &Manager,
        bytes: &[u8],
        registration: Registration,
        central_directory: Option<Directory>,
    ) -> AppResult<Snapshot> {
        let native_generation = self.platform.review_generation();
        self.uninstall_available(&registration.id)?;
        let _exclusive_activity =
            super::uninstall::storage::Activity::acquire(&self.root()?, &registration.id, true)?;
        let verified = package::verify(bytes, &registration).map_err(io::failure)?;
        let target = self.platform.target()?;
        verified
            .compatible(&target, env!("CARGO_PKG_VERSION"))
            .map_err(io::failure)?;
        io::manage(manager, || {
            let store = self.store()?;
            ensure_newer(&store, &registration, &verified.version)?;
            let keys: Vec<_> = self
                .state
                .lock()
                .unwrap()
                .loaded
                .iter()
                .filter(|(_, value)| value.component.manifest().plugin_id == verified.id)
                .map(|(key, _)| key.clone())
                .collect();
            for key in &keys {
                manager.remove_component(key);
            }
            if manager
                .component_snapshots()
                .iter()
                .any(|p| keys.contains(&p.key) && p.running)
            {
                return Err(AppError::invalid_argument("旧组件正在退出，请稍后重新安装"));
            }
            store
                .install(bytes, &registration, &target, env!("CARGO_PKG_VERSION"))
                .map_err(io::failure)?;
            let central = if let Some(directory) = central_directory {
                local_state::save_directory(&self.root()?, &directory)?;
                Some(directory)
            } else {
                local_state::save_local_registration(&self.root()?, registration.clone())?;
                local_state::load_directory(&self.root()?)?
            };
            self.state.lock().unwrap().directory =
                local_state::effective_directory(&self.root()?, central)?;
            self.rescan(manager, &store)
        })?;
        if self
            .platform
            .check_review_generation(native_generation.as_deref())
            .is_err()
        {
            return Err(AppError {
                code: "plugin_install_result_unconfirmed",
                message:
                    "安装期间原生宿主已变化，结果未确认；请重新读取已安装版本，不会自动重放安装"
                        .into(),
                details: None,
            });
        }
        Ok(self.snapshot(manager))
    }
}
