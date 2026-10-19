//! Native foreground admission for the component host; never VPN ownership.
use super::*;
use serde::Serialize;
use znet_plugin_sandbox::contract::{Capability, Os};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub state: &'static str,
    pub target: Option<Target>,
    pub generation: String,
    pub execution_allowed: bool,
    pub external_installation_allowed: bool,
    pub isolation: &'static str,
    pub secure_storage_adapter: bool,
    pub reason: Option<&'static str>,
}
struct State {
    target: Option<Target>,
    generation: u64,
    foreground: bool,
    pending: bool,
    failed: bool,
    secure_vault: bool,
}
pub(super) struct Context {
    mobile: bool,
    state: Mutex<State>,
}
impl Default for Context {
    fn default() -> Self {
        let mobile = cfg!(any(target_os = "android", target_os = "ios"));
        Self {
            mobile,
            state: Mutex::new(State {
                target: if mobile {
                    None
                } else {
                    Target::native_desktop().ok()
                },
                generation: 0,
                foreground: !mobile,
                pending: mobile,
                failed: false,
                secure_vault: !mobile,
            }),
        }
    }
}
pub(super) fn unavailable(reason: &'static str) -> AppError {
    AppError {
        code: "plugin_platform_unavailable",
        message: reason.into(),
        details: None,
    }
}
pub(super) fn unsupported_mobile_capability(capability: Capability) -> Option<&'static str> {
    match capability {
        Capability::RuntimeProtectedLoad => Some("当前移动宿主尚未接入受保护材料的真实运行加载"),
        Capability::FilesSelectionRead | Capability::FilesSelectionWrite => {
            Some("移动插件尚未接入绑定原组件授权的系统文件选择票据")
        }
        Capability::BrowserCallback => Some("移动浏览器回调尚未接入原生返回与后台恢复"),
        Capability::TasksSchedule => Some("移动插件调度尚未接入原生前台协调与系统延期状态"),
        Capability::PersistentSecretsRead
        | Capability::PersistentSecretsWrite
        | Capability::CryptoDeviceUse => {
            Some("当前原生安全存储接口尚未确认，不能降级为普通文件主密钥")
        }
        _ => None,
    }
}
impl Context {
    pub(super) fn unsupported_reason(&self, request: &Request) -> Option<&'static str> {
        if !self.mobile {
            return None;
        }
        if matches!(
            request.capability,
            Capability::PersistentSecretsRead
                | Capability::PersistentSecretsWrite
                | Capability::CryptoDeviceUse
        ) && self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .secure_vault
        {
            return None;
        }
        unsupported_mobile_capability(request.capability)
    }
    pub(super) fn check_review_generation(&self, generation: Option<&str>) -> AppResult<()> {
        if self.mobile && self.review_generation().as_deref() != generation {
            return Err(unavailable("原生插件宿主已变化，请重新审阅后操作"));
        }
        self.require_foreground()
    }
    pub(super) fn review_generation(&self) -> Option<String> {
        if self.mobile {
            Some(
                self.state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .generation
                    .to_string(),
            )
        } else {
            None
        }
    }
    pub(super) fn target(&self) -> AppResult<Target> {
        self.state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?
            .target
            .clone()
            .ok_or_else(|| unavailable("原生插件设备身份尚未核对，请重新检查"))
    }
    pub(super) fn require_foreground(&self) -> AppResult<()> {
        if !self.mobile {
            return Ok(());
        }
        let state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if !state.foreground {
            return Err(AppError {
                code: "plugin_host_suspended",
                message: "插件宿主已在后台暂停，请返回前台后重新操作".into(),
                details: None,
            });
        }
        if state.pending || state.target.is_none() {
            return Err(unavailable("原生插件设备身份尚未核对，请重新检查"));
        }
        Ok(())
    }
    pub(super) fn execution_restriction(&self) -> Option<&'static str> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if self.mobile
            && state
                .target
                .as_ref()
                .is_some_and(|target| target.os == Os::Ios)
            && !cfg!(debug_assertions)
        {
            Some("当前 iOS 发行形态尚未验收外部插件分发；仅 Debug 宿主开放显式审阅的开发插件包")
        } else {
            None
        }
    }
    pub(super) fn require_installation(&self) -> AppResult<()> {
        self.require_foreground()?;
        match self.execution_restriction() {
            Some(reason) => Err(unavailable(reason)),
            None => Ok(()),
        }
    }
    pub(super) fn snapshot(&self) -> Option<Snapshot> {
        if !self.mobile {
            return None;
        }
        let restriction = self.execution_restriction();
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let phase = if state.pending {
            "starting"
        } else if !state.foreground {
            "suspended"
        } else if state.failed || state.target.is_none() {
            "unavailable"
        } else if restriction.is_some() {
            "restricted"
        } else {
            "ready"
        };
        let reason = match phase {
            "starting" => Some("正在核对原生设备、ABI 与插件执行宿主"),
            "suspended" => Some("后台停止新插件调用并取消旧结果；在途原生操作可能仍在排空"),
            "unavailable" => Some("原生设备身份读取未确认，插件执行与安装暂不可用"),
            "restricted" => restriction,
            _ => None,
        };
        Some(Snapshot {
            state: phase,
            target: state.target.clone(),
            generation: state.generation.to_string(),
            execution_allowed: phase == "ready",
            external_installation_allowed: phase == "ready",
            isolation: "bounded-in-process-interpreter",
            secure_storage_adapter: state.secure_vault,
            reason,
        })
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    fn request_resume(&self) -> AppResult<Option<u64>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if state.foreground && (state.pending || state.target.is_some()) {
            return Ok(None);
        }
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unavailable("插件宿主代次已耗尽"))?;
        state.foreground = true;
        state.pending = true;
        state.failed = false;
        Ok(Some(state.generation))
    }
    #[cfg(test)]
    fn commit_resume(&self, generation: u64, target: AppResult<Target>) -> AppResult<bool> {
        self.commit_profile(generation, target, false)
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    fn request_recheck(&self) -> AppResult<Option<u64>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if !state.foreground {
            return Err(unavailable("应用仍在原生后台，请返回前台后重新核对"));
        }
        if state.pending || state.target.is_some() {
            return Ok(None);
        }
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unavailable("插件宿主代次已耗尽"))?;
        state.pending = true;
        state.failed = false;
        Ok(Some(state.generation))
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    fn commit_profile(
        &self,
        generation: u64,
        target: AppResult<Target>,
        secure_vault: bool,
    ) -> AppResult<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if !state.foreground || state.generation != generation {
            return Ok(false);
        }
        state.pending = false;
        match target {
            Ok(target) => {
                state.target = Some(target);
                state.failed = false;
                state.secure_vault = secure_vault;
            }
            Err(_) => {
                state.target = None;
                state.failed = true;
                state.secure_vault = false;
            }
        }
        Ok(true)
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    fn suspend(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.foreground = false;
        state.pending = false;
        state.generation = state.generation.saturating_add(1);
    }
}
impl Host {
    pub(super) fn dispatch_executor(&self) -> AppResult<znet_plugin_sandbox::runtime::Executor> {
        self.platform.require_foreground()?;
        if let Some(reason) = self.platform.execution_restriction() {
            return Err(unavailable(reason));
        }
        Ok(self.executor_shutdown_handle())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(crate) fn suspend_native(&self) {
        // Native suspension must not wait for an installation/catalog network
        // operation holding the management mutex. Seal admission immediately.
        self.platform.suspend();
        self.native_installs.clear();
        self.uninstalls.invalidate_plans();
        self.close_page_documents();
        self.executor_shutdown_handle()
            .close(std::time::Duration::ZERO);
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn resume_native(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::state::app_state::AppState>();
    let host = state.plugins();
    let Ok(Some(generation)) = host.platform.request_resume() else {
        return;
    };
    query_native_profile(app, generation);
}
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn recheck_native(app: &tauri::AppHandle) -> AppResult<()> {
    use tauri::Manager;
    let state = app.state::<crate::state::app_state::AppState>();
    if let Some(generation) = state.plugins().platform.request_recheck()? {
        query_native_profile(app, generation);
    }
    Ok(())
}
#[cfg(any(target_os = "android", target_os = "ios"))]
fn query_native_profile(app: &tauri::AppHandle, generation: u64) {
    use tauri::{Emitter, Manager};
    use tauri_plugin_znet_native_io::NativeIoExt;
    let _ = app.emit("plugins:changed", ());
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let environment = app.native_io().host_environment();
        let secure_vault = environment
            .as_ref()
            .is_ok_and(|value| value.secure_vault_api);
        let target = environment
            .map_err(|_| unavailable("原生设备身份读取未确认"))
            .and_then(|environment| {
                let device = match environment.device_class.as_str() {
                    "phone" => znet_plugin_sandbox::contract::DeviceClass::Phone,
                    "tablet" => znet_plugin_sandbox::contract::DeviceClass::Tablet,
                    _ => return Err(unavailable("原生设备形态不支持插件")),
                };
                Target::native_for_device(device)
                    .map_err(|_| unavailable("当前设备与 ABI 未适配插件"))
            });
        let state = app.state::<crate::state::app_state::AppState>();
        let host = state.plugins();
        {
            let _operation = host
                .operation
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !matches!(
                host.platform
                    .commit_profile(generation, target, secure_vault),
                Ok(true)
            ) {
                return;
            }
            let reader = if secure_vault {
                let native_app = app.clone();
                Some(Arc::new(move |create| {
                    native_app
                        .native_io()
                        .plugin_vault_master(create)
                        .map_err(|_| super::vault::unavailable())
                }) as super::vault::KeyReader)
            } else {
                None
            };
            *host
                .native_vault
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = reader;
            let mut executor = host
                .executor
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !executor.snapshot().accepting {
                *executor = Default::default();
            }
            host.page_documents.reopen_empty();
        }
        if let Ok(_operation) = host.operation_guard() {
            let _ = host.checked_rescan(state.capabilities());
        }
        host.start_enabled_with_app(state.capabilities(), Some(app.clone()));
        record_native_smoke(&app);
        let _ = app.emit("plugins:changed", ());
    });
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn suspend_native(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    app.state::<crate::state::app_state::AppState>()
        .plugins()
        .suspend_native();
    record_native_smoke(app);
    let _ = app.emit("plugins:changed", ());
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn record_native_smoke(app: &tauri::AppHandle) {
    #[cfg(all(debug_assertions, target_os = "ios"))]
    {
        use tauri::Manager;
        let nonce = std::env::var("ZNET_IOS_UI_SMOKE_NONCE").unwrap_or_default();
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return;
        }
        let state = app.state::<crate::state::app_state::AppState>();
        let host = state.plugins();
        let value = serde_json::json!({"schemaVersion":1,"nonce":nonce,"pid":std::process::id(),"platform":host.platform.snapshot(),"executorAccepting":host.executor_shutdown_handle().snapshot().accepting});
        if let Ok(root) = app.path().app_cache_dir() {
            let result = std::fs::create_dir_all(&root).and_then(|()| {
                let bytes = serde_json::to_vec(&value).map_err(std::io::Error::other)?;
                let temporary = root.join("znet-plugin-lifecycle-smoke.tmp");
                std::fs::write(&temporary, bytes)?;
                std::fs::rename(temporary, root.join("znet-plugin-lifecycle-smoke.json"))
            });
            if result.is_err() {
                crate::services::file_logger::line(
                    "native plugin lifecycle smoke receipt unavailable",
                );
            }
        }
    }
    #[cfg(not(all(debug_assertions, target_os = "ios")))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;
    use znet_plugin_sandbox::contract::{Arch, DeviceClass};
    fn mobile() -> Context {
        Context {
            mobile: true,
            state: Mutex::new(State {
                target: None,
                generation: 0,
                foreground: false,
                pending: true,
                failed: false,
                secure_vault: false,
            }),
        }
    }
    fn phone() -> Target {
        Target {
            os: Os::Android,
            arch: Arch::Aarch64,
            device: DeviceClass::Phone,
        }
    }
    #[test]
    fn secure_sdk_requires_fresh_native_api_admission_and_keeps_other_gates() {
        let context = mobile();
        let request = Request {
            capability: Capability::PersistentSecretsRead,
            scope: "self".into(),
        };
        assert!(context.unsupported_reason(&request).is_some());
        let first = context.request_resume().unwrap().unwrap();
        assert!(context.commit_profile(first, Ok(phone()), true).unwrap());
        assert!(context.unsupported_reason(&request).is_none());
        assert!(context
            .unsupported_reason(&Request {
                capability: Capability::FilesSelectionRead,
                scope: "user".into()
            })
            .is_some());
        context.suspend();
        let next = context.request_resume().unwrap().unwrap();
        assert!(context.commit_profile(next, Ok(phone()), false).unwrap());
        assert!(context.unsupported_reason(&request).is_some());
        assert!(!context.commit_profile(first, Ok(phone()), true).unwrap());
        assert!(context.unsupported_reason(&request).is_some());
    }
    #[test]
    fn ui_recheck_cannot_create_foreground_authority_or_commit_after_suspend() {
        let context = mobile();
        assert!(context.request_recheck().is_err());
        assert_eq!(context.snapshot().unwrap().state, "starting");
        let native = context.request_resume().unwrap().unwrap();
        assert!(context.request_recheck().unwrap().is_none());
        context
            .commit_resume(native, Err(unavailable("missing")))
            .unwrap();
        let query = context.request_recheck().unwrap().unwrap();
        context.suspend();
        assert!(context.request_recheck().is_err());
        assert!(!context.commit_resume(query, Ok(phone())).unwrap());
        assert_eq!(context.snapshot().unwrap().state, "suspended");
    }
    #[test]
    fn background_and_resume_reject_a_late_native_profile() {
        let context = mobile();
        let first = context.request_resume().unwrap().unwrap();
        context.suspend();
        let second = context.request_resume().unwrap().unwrap();
        assert!(!context.commit_resume(first, Ok(phone())).unwrap());
        assert!(context.require_foreground().is_err());
        assert!(context.commit_resume(second, Ok(phone())).unwrap());
        let reviewed = context.review_generation().unwrap();
        assert!(context.check_review_generation(Some(&reviewed)).is_ok());
        assert_eq!(context.snapshot().unwrap().state, "ready");
        context.suspend();
        assert!(context.check_review_generation(Some(&reviewed)).is_err());
        let resumed = context.request_resume().unwrap().unwrap();
        assert!(context.commit_resume(resumed, Ok(phone())).unwrap());
        assert!(context.check_review_generation(Some(&reviewed)).is_err());
        assert!(context.check_review_generation(None).is_err());
        context.suspend();
        assert_eq!(
            context.require_foreground().unwrap_err().code,
            "plugin_host_suspended"
        );
        assert_eq!(context.snapshot().unwrap().state, "suspended");
    }
    #[test]
    fn missing_device_facts_do_not_become_a_phone_or_authoritative_empty_catalog() {
        let context = mobile();
        let generation = context.request_resume().unwrap().unwrap();
        context
            .commit_resume(generation, Err(unavailable("unknown")))
            .unwrap();
        let snapshot = context.snapshot().unwrap();
        assert_eq!(snapshot.state, "unavailable");
        assert!(snapshot.target.is_none());
        assert!(!snapshot.execution_allowed);
        assert!(context.require_installation().is_err());
    }
    #[test]
    fn unadapted_sdk_roles_are_separate_from_independent_io() {
        for capability in [
            Capability::FilesSelectionRead,
            Capability::RuntimeProtectedLoad,
            Capability::BrowserCallback,
            Capability::PersistentSecretsWrite,
        ] {
            assert!(unsupported_mobile_capability(capability).is_some());
        }
        for capability in [
            Capability::StorageWrite,
            Capability::NetworkRequest,
            Capability::SelfRead,
            Capability::CryptoSessionUse,
        ] {
            assert!(unsupported_mobile_capability(capability).is_none());
        }
    }
}
