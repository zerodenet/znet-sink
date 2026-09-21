use super::*;
use crate::services::common::now_unix_ms;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use tauri::{Emitter as _, Manager as _};
use tauri_plugin_notification::NotificationExt as _;
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

fn deliver_system_plugin_notification(app: &tauri::AppHandle, value: &serde_json::Value) {
    use tauri::plugin::PermissionState;

    let notification = app.notification();
    let permission = match notification.permission_state() {
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => {
            notification.request_permission().ok()
        }
        Ok(state) => Some(state),
        Err(_) => None,
    };
    if permission != Some(PermissionState::Granted) {
        return;
    }
    let title = value
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ZNet Sink");
    let body = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("插件有一条新通知");
    let _ = notification.builder().title(title).body(body).show();
}

impl Host {
    pub fn refresh(&self, manager: &Manager) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
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
        let _operation = self.operation.lock().unwrap();
        self.checked_rescan(manager)?;
        if grants.iter().any(|request| !supported(request)) {
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
        let _operation = self.operation.lock().unwrap();
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
        let _operation = self.operation.lock().unwrap();
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
        let (component, authority, configuration) = {
            let _operation = self.operation.lock().unwrap();
            self.checked_rescan(manager)?;
            self.current_review(manager, &review)?;
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
            )
        };
        runtime::execute_for_host_with_input(
            &component,
            &authority,
            serde_json::json!({ "configuration": configuration }),
            Arc::new(AtomicBool::new(false)),
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|error| {
            use znet_plugin_sandbox::contract::Error;
            AppError::invalid_argument(match error {
                Error::Revoked | Error::Expired | Error::Disabled | Error::PermissionDenied => {
                    "插件权限已失效，请重新查看权限并确认"
                }
                Error::Busy => "此组件正在运行，请稍后重试",
                Error::Deadline | Error::BudgetExceeded => "插件运行超过资源限制，已停止",
                Error::Cancelled => "插件运行已停止",
                _ => "插件运行失败，请检查兼容性或联系发布者",
            })
        })
    }
    pub fn invoke(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        action: String,
        payload: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        if action.is_empty()
            || action.len() > 128
            || !action
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || serde_json::to_vec(&payload).map_err(io::failure)?.len() > 16 * 1024
        {
            return Err(AppError::invalid_argument("插件页面操作参数无效"));
        }
        let (component, authority, configuration, publisher, plugin_state) = {
            let _operation = self.operation.lock().unwrap();
            self.checked_rescan(manager)?;
            let key = format!("{plugin_id}/{component_id}");
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
            (
                loaded.component.clone(),
                loaded.authority.clone().ok_or_else(stale)?,
                configuration,
                publisher,
                plugin_state,
            )
        };
        let digest = component.digest().to_owned();
        let output = runtime::execute_for_host_with_input(
            &component,
            &authority,
            serde_json::json!({
                "configuration": configuration,
                "state": plugin_state,
                "invocation": { "action": action, "payload": payload }
            }),
            Arc::new(AtomicBool::new(false)),
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|error| {
            use znet_plugin_sandbox::contract::Error;
            AppError::invalid_argument(match error {
                Error::Revoked | Error::Expired | Error::Disabled | Error::PermissionDenied => {
                    "插件尚未启用，或当前操作没有所需权限"
                }
                Error::Busy => "插件正在处理另一个操作，请稍后重试",
                Error::Deadline | Error::BudgetExceeded => "插件操作超过资源限制，已停止",
                Error::Cancelled => "插件操作已停止",
                _ => "插件操作失败，请在插件页面查看说明",
            })
        })?;
        let Some(marker) = output
            .as_object()
            .and_then(|object| object.get("znet_plugin_result"))
        else {
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
        let _operation = self.operation.lock().unwrap();
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        if loaded.component.digest() != digest || !loaded.consent.enabled {
            return Err(stale());
        }
        drop(state);
        namespace::apply_runtime_updates(
            &self.root()?,
            &publisher,
            plugin_id,
            envelope.state_updates,
        )?;
        Ok(envelope.value)
    }

    /// Execute a declared scheduled action with the same typed SDK surface as
    /// signed management pages. The scheduler and host stay business-neutral;
    /// the plugin owns the action and protocol.
    pub fn invoke_scheduled(
        &self,
        app: tauri::AppHandle,
        plugin_id: &str,
        component_id: &str,
        action: String,
        payload: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        if !action.starts_with("lifecycle.scheduled.") {
            return Err(AppError::invalid_argument("插件后台动作无效"));
        }
        let (component, authority, configuration, publisher, plugin_state) = {
            let _operation = self.operation.lock().unwrap();
            let manager = app.state::<crate::state::app_state::AppState>();
            self.checked_rescan(manager.capabilities())?;
            let key = format!("{plugin_id}/{component_id}");
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
            )
        };
        let digest = component.digest().to_owned();
        let dispatch_app = app.clone();
        let dispatch_plugin = plugin_id.to_owned();
        let dispatch_component = component_id.to_owned();
        let dispatcher: runtime::HostSdkDispatcher = Arc::new(move |lease, input| {
            let result = (|| -> AppResult<serde_json::Value> {
                let call: Call = serde_json::from_str(input)
                    .map_err(|_| AppError::invalid_argument("插件后台 SDK 请求格式无效"))?;
                let notification = call.method == Method::NotificationPost;
                let state = dispatch_app.state::<crate::state::app_state::AppState>();
                if call.method == Method::SubscriptionApply {
                    let (managed, request) = state
                        .plugins()
                        .managed_subscription_input_with_lease(&dispatch_plugin, lease, call)?;
                    return tauri::async_runtime::block_on(
                        crate::services::subscription::apply_managed_authorized(
                            dispatch_app.clone(),
                            managed,
                            || lease.check(Some(&request)).map_err(io::failure),
                        ),
                    )
                    .and_then(|profile| serde_json::to_value(profile).map_err(io::failure));
                }
                if call.method == Method::SubscriptionRemove {
                    let (subscription_id, remove_associated_config, request) = state
                        .plugins()
                        .managed_subscription_removal_with_lease(lease, call)?;
                    return tauri::async_runtime::block_on(
                        crate::services::subscription::remove_managed_authorized(
                            dispatch_app.clone(),
                            subscription_id,
                            remove_associated_config,
                            dispatch_plugin.clone(),
                            || lease.check(Some(&request)).map_err(io::failure),
                        ),
                    )
                    .and_then(|outcome| serde_json::to_value(outcome).map_err(io::failure));
                }
                let network_proxy = crate::configuration::preferences::endpoint(state.inner())
                    .ok()
                    .map(|(host, port)| format!("http://{host}:{port}"));
                let value = state.plugins().sdk_call_with_lease(
                    state.capabilities(),
                    &dispatch_plugin,
                    &dispatch_component,
                    lease,
                    call,
                    network_proxy,
                )?;
                if notification {
                    let _ = dispatch_app.emit("plugin:notification", value.clone());
                    deliver_system_plugin_notification(&dispatch_app, &value);
                }
                Ok(value)
            })();
            let reply = match result {
                Ok(value) => serde_json::json!({"version":1,"ok":true,"value":value}),
                Err(error) => serde_json::json!({
                    "version":1,
                    "ok":false,
                    "error":{"code":error.code,"message":error.message}
                }),
            };
            serde_json::to_string(&reply)
                .map_err(|_| znet_plugin_sandbox::contract::Error::InvalidOutput)
        });
        let output = runtime::execute_scheduled_for_host_with_input(
            &component,
            &authority,
            serde_json::json!({
                "configuration": configuration,
                "state": plugin_state,
                "invocation": {
                    "action": action,
                    "payload": payload,
                    "now_unix_ms": now_unix_ms()
                }
            }),
            Arc::new(AtomicBool::new(false)),
            env!("CARGO_PKG_VERSION"),
            dispatcher,
        )
        .map_err(|error| AppError::invalid_argument(format!("插件后台操作失败：{error}")))?;
        let envelope: InvocationEnvelope = serde_json::from_value(output)
            .map_err(|_| AppError::invalid_argument("插件返回的状态更新格式无效"))?;
        if envelope.znet_plugin_result != 1 {
            return Err(AppError::invalid_argument("插件返回了不支持的结果协议"));
        }
        let _operation = self.operation.lock().unwrap();
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        if loaded.component.digest() != digest || !loaded.consent.enabled {
            return Err(stale());
        }
        drop(state);
        namespace::apply_runtime_updates(
            &self.root()?,
            &publisher,
            plugin_id,
            envelope.state_updates,
        )?;
        Ok(envelope.value)
    }
    /// Run declared bounded startup hooks after the app state is available.
    /// A hook receives its plugin's small opaque state snapshot and may return
    /// an atomic state-update envelope. Failures stay isolated to that plugin.
    pub fn start_enabled(&self, manager: &Manager) {
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
                .map(|loaded| {
                    (
                        loaded.component.manifest().plugin_id.clone(),
                        loaded.component.manifest().component_id.clone(),
                    )
                })
                .collect()
        };
        let mut notices = Vec::new();
        for (plugin_id, component_id) in lifecycle {
            if let Err(error) = self.invoke(
                manager,
                &plugin_id,
                &component_id,
                "lifecycle.host_start".into(),
                serde_json::Value::Null,
            ) {
                notices.push(format!(
                    "{plugin_id}：后台启动检查失败（{}）",
                    error.message
                ));
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
        let _operation = self.operation.lock().unwrap();
        self.checked_rescan(manager)?;
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
        manager.revoke_component(key);
        configuration::save(&self.root()?, key, values)?;
        self.rescan(manager, &self.store()?)?;
        Ok(self.snapshot(manager))
    }
    pub fn uninstall(&self, manager: &Manager, id: &str) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        let (keys, publisher) = {
            let state = self.state.lock().unwrap();
            (
                state
                    .loaded
                    .iter()
                    .filter(|(_, value)| value.component.manifest().plugin_id == id)
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>(),
                state
                    .loaded
                    .values()
                    .find(|value| value.component.manifest().plugin_id == id)
                    .map(|value| value.publisher_fingerprint.clone()),
            )
        };
        for key in keys {
            manager.remove_component(&key);
        }
        io::manage(manager, || self.store()?.uninstall(id).map_err(io::failure))?;
        configuration::remove_plugin(&self.root()?, id)?;
        local_state::remove_plugin(&self.root()?, id)?;
        local_state::remove_local_registration(&self.root()?, id)?;
        self.remove_plugin_schedules(id)?;
        self.callbacks.cancel_plugin(id);
        self.sensitive.clear_plugin(id);
        self.state.lock().unwrap().directory = local_state::effective_directory(
            &self.root()?,
            local_state::load_directory(&self.root()?)?,
        )?;
        if let Some(publisher) = publisher {
            namespace::remove_plugin(&self.root()?, &publisher, id)?;
        }
        self.state
            .lock()
            .unwrap()
            .loaded
            .retain(|_, value| value.component.manifest().plugin_id != id);
        self.state.lock().unwrap().pages.remove(id);
        Ok(self.snapshot(manager))
    }
    fn selected_package(&self, manager: &Manager, path: String) -> AppResult<Vec<u8>> {
        let selected = znet_client_capabilities::files::Selection::from_user_path(path.into());
        let lease = io::lease(manager, BTreeSet::from([selected.permission()]))?;
        znet_client_capabilities::files::read(&lease, &selected, MAX_PACKAGE_BYTES)
            .and_then(|resource| resource.take(&lease))
            .map_err(io::failure)
    }
    pub fn preview_install(&self, manager: &Manager, path: String) -> AppResult<InstallReview> {
        let _operation = self.operation.lock().unwrap();
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
        let _operation = self.operation.lock().unwrap();
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
                // Legacy v1 packages did not embed a key. They can still migrate
                // through a cached/public publisher key, but new unpublished
                // packages must use v2 so local installation is self-contained.
                let central = match local_state::load_directory(&self.root()?)? {
                    Some(cached) => cached,
                    None => io::directory(manager)?,
                };
                central.find(id).map_err(|_| {
                    AppError::invalid_argument(
                        "此插件包未携带发布者公钥，且插件中心没有可用于验签的登记；请发布者重新生成带发布者登记的 v2/v3 包",
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
        let target = Target::native_desktop().map_err(io::failure)?;
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
        if review.requires_approval && approval_digest != Some(review.candidate_digest.as_str()) {
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
        let review = self.preview_bytes(bytes, &directory, id)?;
        if review.requires_approval && approval_digest != Some(review.candidate_digest.as_str()) {
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
        let verified = package::verify(bytes, &registration).map_err(io::failure)?;
        let target = Target::native_desktop().map_err(io::failure)?;
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
        Ok(self.snapshot(manager))
    }
}
