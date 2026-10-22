//! Native foreground admission for the component host; never VPN ownership.
use super::*;
use serde::Serialize;
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

static NEXT_CONTEXT: AtomicU64 = AtomicU64::new(1);
fn context_lifetime() -> Option<String> {
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).ok()?;
    Some(nonce.iter().map(|byte| format!("{byte:02x}")).collect())
}
thread_local! { static OS_ADMISSION: RefCell<Option<BackgroundPermit>> = const { RefCell::new(None) }; }

/// Host-only authority for one bounded OS job. It is neither a plugin grant
/// nor a foreground flag. Every VM/SDK worker must enter its own scoped permit.
#[derive(Clone)]
pub(super) struct BackgroundPermit {
    context: u64,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
pub(super) struct BackgroundGuard {
    previous: Option<BackgroundPermit>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Drop for BackgroundGuard {
    fn drop(&mut self) {
        OS_ADMISSION.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
impl BackgroundPermit {
    pub(super) fn check(&self) -> AppResult<()> {
        if self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            self.cancelled.store(true, Ordering::Release);
            return Err(AppError {
                code: "plugin_host_suspended",
                message: "系统后台作业已停止或达到运行预算；在途结果需核对".into(),
                details: None,
            });
        }
        Ok(())
    }
    pub(super) fn enter(&self) -> AppResult<BackgroundGuard> {
        self.check()?;
        let previous = OS_ADMISSION.with(|slot| slot.replace(Some(self.clone())));
        Ok(BackgroundGuard {
            previous,
            _thread: std::marker::PhantomData,
        })
    }
}

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
    pub task_scheduling: SchedulingSnapshot,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulingSnapshot {
    pub mode: &'static str,
    pub state: &'static str,
    pub reason: &'static str,
}
struct State {
    target: Option<Target>,
    generation: u64,
    foreground: bool,
    pending: bool,
    failed: bool,
    secure_vault: bool,
    background: Option<BackgroundPermit>,
}
pub(super) struct Context {
    identity: u64,
    lifetime: Option<String>,
    mobile: bool,
    state: Mutex<State>,
}
impl Default for Context {
    fn default() -> Self {
        let mobile = cfg!(any(target_os = "android", target_os = "ios"));
        Self {
            identity: NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed),
            lifetime: context_lifetime(),
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
                background: None,
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
        Capability::PersistentSecretsRead
        | Capability::PersistentSecretsWrite
        | Capability::CryptoDeviceUse => {
            Some("当前原生安全存储接口尚未确认，不能降级为普通文件主密钥")
        }
        _ => None,
    }
}
#[cfg(test)]
pub(super) fn android_context_for_test() -> Context {
    let mut context = Context::default();
    context.mobile = true;
    {
        let mut state = context.state.lock().unwrap();
        state.foreground = false;
        state.pending = false;
        state.target = None;
    }
    context
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
        if !self.mobile {
            return None;
        }
        let generation = self.state.lock().ok()?.generation;
        Some(format!("{}:{generation}", self.lifetime.as_deref()?))
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
        if self.lifetime.is_none() {
            return Err(unavailable("无法建立安全原生宿主身份"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if !state.foreground {
            let admitted = OS_ADMISSION.with(|slot| {
                slot.borrow().as_ref().is_some_and(|permit| {
                    permit.context == self.identity
                        && permit.generation == state.generation
                        && state
                            .background
                            .as_ref()
                            .is_some_and(|active| active.generation == permit.generation)
                        && permit.check().is_ok()
                })
            });
            if admitted && state.target.is_some() && !state.pending {
                return Ok(());
            }
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
            generation: self
                .lifetime
                .as_ref()
                .map(|lifetime| format!("{lifetime}:{}", state.generation))
                .unwrap_or_else(|| "unavailable".into()),
            execution_allowed: phase == "ready",
            external_installation_allowed: phase == "ready",
            isolation: "bounded-in-process-interpreter",
            secure_storage_adapter: state.secure_vault,
            reason,
            task_scheduling: SchedulingSnapshot {
                mode: if cfg!(target_os = "android") {
                    "android-job-scheduler"
                } else {
                    "foreground-only"
                },
                state: if phase == "ready" {
                    "available"
                } else {
                    "deferred"
                },
                reason: if cfg!(target_os = "android") {
                    "Android 系统作业在后台核对到期任务（最短 15 分钟系统周期）；省电、锁屏、网络和强行停止可能推迟或阻止运行。单次派发窗口 90 秒，在途 IO 可能排空；结果未确认不会自动重放。"
                } else {
                    "仅在应用前台核对到期任务；后台执行尚未适配，不保证执行时间。结果未确认的任务不会自动重试"
                },
            },
        })
    }
    #[cfg(any(target_os = "android", test))]
    pub(super) fn begin_background(
        &self,
        target: Target,
        secure_vault: bool,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<BackgroundPermit> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("无法核对原生插件宿主"))?;
        if self.lifetime.is_none()
            || state.foreground
            || state.background.is_some()
            || target.os != Os::Android
            || cancelled.load(Ordering::Acquire)
        {
            return Err(unavailable(
                "前台或另一系统作业已持有插件执行；本次后台运行推迟",
            ));
        }
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unavailable("插件宿主代次已耗尽"))?;
        let permit = BackgroundPermit {
            context: self.identity,
            generation: state.generation,
            cancelled,
            deadline: Instant::now() + std::time::Duration::from_secs(85),
        };
        state.target = Some(target);
        state.pending = false;
        state.failed = false;
        state.secure_vault = secure_vault;
        state.background = Some(permit.clone());
        Ok(permit)
    }
    #[cfg(any(target_os = "android", test))]
    pub(super) fn end_background(&self, permit: &BackgroundPermit) -> bool {
        permit.cancelled.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state
            .background
            .as_ref()
            .is_none_or(|active| active.generation != permit.generation)
        {
            return false;
        }
        state.background = None;
        state.generation = state.generation.saturating_add(1);
        true
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
        if let Some(background) = state.background.take() {
            background.cancelled.store(true, Ordering::Release);
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
    fn suspend(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // A late UI disappearance must not cancel a JobService that already
        // acquired its own OS lease while the UI was absent.
        if !state.foreground && state.background.is_some() {
            return false;
        }
        if let Some(background) = state.background.take() {
            background.cancelled.store(true, Ordering::Release);
        }
        state.foreground = false;
        state.pending = false;
        state.generation = state.generation.saturating_add(1);
        true
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
        if !self.platform.suspend() {
            return;
        }
        self.scheduler.notify();
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
        super::scheduler::resume(app.clone());
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
pub(crate) fn shutdown_native(app: &tauri::AppHandle) {
    #[cfg(target_os = "ios")]
    use tauri::Manager;
    suspend_native(app);
    // Android Activity exit is not process/service shutdown. Keep the shared
    // queue owner usable by later JobService wakes and a recreated Activity.
    #[cfg(target_os = "ios")]
    app.state::<crate::state::app_state::AppState>()
        .plugins()
        .scheduler
        .close();
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
            identity: NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed),
            lifetime: context_lifetime(),
            mobile: true,
            state: Mutex::new(State {
                target: None,
                generation: 0,
                foreground: false,
                pending: true,
                failed: false,
                secure_vault: false,
                background: None,
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
    fn native_reviews_cannot_rebind_to_a_new_process_lifetime() {
        let first = mobile();
        let second = mobile();
        let a = first.request_resume().unwrap().unwrap();
        let b = second.request_resume().unwrap().unwrap();
        first.commit_resume(a, Ok(phone())).unwrap();
        second.commit_resume(b, Ok(phone())).unwrap();
        assert_eq!(a, b); // Counter reuse alone must not restore old authority.
        assert!(second
            .check_review_generation(first.review_generation().as_deref())
            .is_err());
        let mut unavailable_context = mobile();
        unavailable_context.lifetime = None;
        assert!(unavailable_context
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .is_err());
    }
    #[test]
    fn os_job_admission_is_thread_scoped_and_never_opens_foreground_calls() {
        let context = Arc::new(mobile());
        let cancelled = Arc::new(AtomicBool::new(false));
        let permit = context
            .begin_background(phone(), false, cancelled.clone())
            .unwrap();
        assert!(context.require_foreground().is_err());
        {
            let _entered = permit.enter().unwrap();
            assert!(context.require_foreground().is_ok());
            let other = context.clone();
            assert!(
                std::thread::spawn(move || other.require_foreground().is_err())
                    .join()
                    .unwrap()
            );
            let other = context.clone();
            let transferred = permit.clone();
            assert!(std::thread::spawn(move || {
                let _scope = transferred.enter().unwrap();
                other.require_foreground().is_ok()
            })
            .join()
            .unwrap());
        }
        assert!(context.require_foreground().is_err());
        cancelled.store(true, Ordering::Release);
        assert!(permit.enter().is_err());
        assert!(context.end_background(&permit));
    }
    #[test]
    fn late_activity_exit_does_not_cancel_existing_os_owned_run() {
        let context = mobile();
        let permit = context
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert!(!context.suspend());
        let _scope = permit.enter().unwrap();
        assert!(context.require_foreground().is_ok());
        assert!(context.end_background(&permit));
    }
    #[test]
    fn returning_foreground_invalidates_os_job_before_new_authority() {
        let context = mobile();
        let permit = context
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .unwrap();
        let generation = context.request_resume().unwrap().unwrap();
        assert!(permit.enter().is_err());
        assert!(!context.end_background(&permit));
        assert!(context.commit_resume(generation, Ok(phone())).unwrap());
        assert!(context.require_foreground().is_ok());
    }
    #[test]
    fn os_job_deadline_and_other_context_never_inherit_admission() {
        let context = mobile();
        let other = mobile();
        let mut permit = context
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .unwrap();
        {
            let _scope = permit.enter().unwrap();
            assert!(other.require_foreground().is_err());
        }
        permit.deadline = Instant::now();
        assert!(permit.enter().is_err());
        assert!(context.require_foreground().is_err());
    }
    #[test]
    fn native_background_does_not_advance_tasks_and_resume_does_not_replay_unconfirmed_dispatch() {
        for dispatched in [false, true] {
            let (root, mut host, manager) = super::super::tests::setup_component(
                "42",
                serde_json::json!({"capability":"tasks.schedule","scope":"self"}),
            );
            host.platform = mobile();
            let first = host.platform.request_resume().unwrap().unwrap();
            host.platform.commit_resume(first, Ok(phone())).unwrap();
            host.checked_rescan(&manager).unwrap();
            let row = host.snapshot(&manager).components.remove(0);
            host.authorize(
                &manager,
                row.review.unwrap(),
                vec![Request {
                    capability: Capability::TasksSchedule,
                    scope: "self".into(),
                }],
            )
            .unwrap();
            host.schedule_put(
                "org.example.plugin",
                "identity",
                "refresh".into(),
                "refresh".into(),
                300,
            )
            .unwrap();
            let old = host.take_due(&manager, u64::MAX).unwrap().remove(0);
            if dispatched {
                let _operation = host.operation_guard().unwrap();
                host.mark_scheduled_dispatch(&manager, &old, 1234).unwrap();
            }
            let file = root.path().join("schedules.json");
            let before = std::fs::read(&file).unwrap();
            host.platform.suspend();
            assert!(host.take_due(&manager, u64::MAX).is_err());
            assert_eq!(std::fs::read(&file).unwrap(), before);
            let resumed = host.platform.request_resume().unwrap().unwrap();
            host.platform.commit_resume(resumed, Ok(phone())).unwrap();
            assert!(host.take_due(&manager, u64::MAX - 1).unwrap().is_empty());
            let tasks = serde_json::to_value(
                host.schedule_list("org.example.plugin", "identity")
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                tasks[0]["lastAttempt"]["state"],
                if dispatched {
                    "unconfirmed"
                } else {
                    "cancelled"
                }
            );
            {
                let _operation = host.operation_guard().unwrap();
                assert!(host.checked_scheduled_due(&manager, &old).is_err());
            }
            assert_eq!(
                host.take_due(&manager, u64::MAX).unwrap().len(),
                usize::from(!dispatched)
            );
            let snapshot = host.platform.snapshot().unwrap();
            assert_eq!(snapshot.task_scheduling.mode, "foreground-only");
            assert!(snapshot.task_scheduling.reason.contains("后台执行尚未适配"));
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
