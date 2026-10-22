//! Android JobService JNI entry. No Activity, WebView or Tauri runtime handle is
//! required to restore the same Rust services later used by the foreground UI.
use crate::{
    errors::{AppError, AppResult},
    state::app_state::AppState,
};
use jni::{
    objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue},
    sys::jstring,
    JNIEnv,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};

static STATE: OnceLock<AppState> = OnceLock::new();
static INITIALIZING: Mutex<()> = Mutex::new(());
static RUNS: OnceLock<Mutex<BTreeMap<String, Arc<AtomicBool>>>> = OnceLock::new();
fn bridge_error() -> AppError {
    AppError::internal("Android 后台服务边界不可用；保留原任务记录")
}

pub(crate) fn shared_state() -> AppResult<AppState> {
    let _initializing = INITIALIZING.lock().map_err(|_| bridge_error())?;
    if let Some(state) = STATE.get() {
        return Ok(state.clone());
    }
    // Fail closed for corrupt data. A cold job must not replace it with empty
    // caches and subsequently overwrite the user's profiles or preferences.
    let dir = crate::services::data_dir()?;
    crate::services::app_config_store::load_or_default(&dir.join("app-config.json"))?;
    crate::services::domain_store::load_all_from_dir(&dir)?;
    use crate::lifecycle::OnPhase;
    let startup = Arc::new(Mutex::new(None));
    crate::lifecycle::phases::ConfigPhase {
        data: startup.clone(),
    }
    .run()
    .map_err(|_| bridge_error())?;
    let data = startup
        .lock()
        .map_err(|_| bridge_error())?
        .take()
        .ok_or_else(bridge_error)?;
    let state = AppState::with_domain_data(
        data.app_config,
        data.domain_data.proxy_configs,
        data.domain_data.subscriptions,
        data.domain_data.rule_sets,
        data.logs,
    );
    STATE.set(state.clone()).map_err(|_| bridge_error())?;
    Ok(state)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn register(id: &str) -> AppResult<Arc<AtomicBool>> {
    let mut runs = RUNS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| bridge_error())?;
    // Keep pre-start stop tombstones; OS stop can race native library loading.
    if let Some(cancelled) = runs.get(id) {
        return Ok(cancelled.clone());
    }
    if runs.len() >= 64 {
        let expired = runs
            .iter()
            .find(|(_, cancelled)| cancelled.load(Ordering::Acquire))
            .map(|(id, _)| id.clone());
        if let Some(expired) = expired {
            runs.remove(&expired);
        } else {
            return Err(bridge_error());
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    runs.insert(id.into(), cancelled.clone());
    Ok(cancelled)
}
fn stop(id: &str) {
    if let Ok(cancelled) = register(id) {
        cancelled.store(true, Ordering::Release);
    }
}
fn clear_exception(env: &mut JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}
fn java_string(env: &mut JNIEnv<'_>, value: JObject<'_>) -> AppResult<String> {
    env.get_string(&JString::from(value))
        .map(|value| value.into())
        .map_err(|_| bridge_error())
}
fn context_profile(
    env: &mut JNIEnv<'_>,
    context: &JObject<'_>,
) -> AppResult<(
    std::path::PathBuf,
    znet_plugin_sandbox::contract::DeviceClass,
)> {
    let package = env
        .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|_| bridge_error())?;
    let package = java_string(env, package)?;
    if package != "org.zerodenet.znetsink"
        && !(cfg!(debug_assertions) && package == "org.zerodenet.znetsink.debug")
    {
        return Err(bridge_error());
    }
    let file = env
        .call_method(context, "getDataDir", "()Ljava/io/File;", &[])
        .and_then(|v| v.l())
        .map_err(|_| bridge_error())?;
    let path = env
        .call_method(file, "getCanonicalPath", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|_| bridge_error())?;
    let path = std::path::PathBuf::from(java_string(env, path)?);
    let resources = env
        .call_method(
            context,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|_| bridge_error())?;
    let configuration = env
        .call_method(
            resources,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|_| bridge_error())?;
    let width = env
        .get_field(configuration, "smallestScreenWidthDp", "I")
        .and_then(|v| v.i())
        .map_err(|_| bridge_error())?;
    let device = if width >= 600 {
        znet_plugin_sandbox::contract::DeviceClass::Tablet
    } else if width > 0 {
        znet_plugin_sandbox::contract::DeviceClass::Phone
    } else {
        return Err(bridge_error());
    };
    Ok((path, device))
}
fn vault_reader(
    vm: jni::JavaVM,
    class: GlobalRef,
    context: GlobalRef,
) -> Arc<dyn Fn(bool) -> AppResult<Option<zeroize::Zeroizing<Vec<u8>>>> + Send + Sync> {
    Arc::new(move |_create| {
        let mut env = vm.attach_current_thread().map_err(|_| bridge_error())?;
        let result = (|| {
            let class: &JClass<'_> = class.as_obj().into();
            let value = env
                .call_static_method(
                    class,
                    "readVault",
                    "(Landroid/content/Context;)[B",
                    &[JValue::Object(context.as_obj())],
                )
                .and_then(|v| v.l())
                .map_err(|_| bridge_error())?;
            if value.is_null() {
                return Ok(None);
            }
            let array = JByteArray::from(value);
            let bytes = env.convert_byte_array(&array).map_err(|_| bridge_error())?;
            // Clear the Java copy as soon as Rust has a zeroizing owner.
            let _ = env.set_byte_array_region(&array, 0, &[0; 32]);
            if bytes.len() != 32 {
                return Err(bridge_error());
            }
            Ok(Some(zeroize::Zeroizing::new(bytes)))
        })();
        clear_exception(&mut env);
        result
    })
}
fn execute(
    env: &mut JNIEnv<'_>,
    class: JClass<'_>,
    context: JObject<'_>,
    id: &str,
) -> AppResult<crate::services::plugins::background::Outcome> {
    if !valid_id(id) {
        return Err(bridge_error());
    }
    let cancelled = register(id)?;
    let result = (|| {
        if cancelled.load(Ordering::Acquire) {
            return Ok(crate::services::plugins::background::Outcome::deferred(
                "stopped-before-start",
            ));
        }
        let (path, device) = context_profile(env, &context)?;
        crate::services::install_mobile_data_dir(path)?;
        crate::services::file_logger::init();
        let state = shared_state()?;
        let target = znet_plugin_sandbox::contract::Target::native_for_device(device)
            .map_err(|_| bridge_error())?;
        let reader = vault_reader(
            env.get_java_vm().map_err(|_| bridge_error())?,
            env.new_global_ref(class).map_err(|_| bridge_error())?,
            env.new_global_ref(context).map_err(|_| bridge_error())?,
        );
        crate::services::plugins::background::run(state, target, reader, cancelled.clone())
    })();
    cancelled.store(true, Ordering::Release);
    if let Ok(mut runs) = RUNS.get_or_init(Default::default).lock() {
        runs.remove(id);
    }
    result
}

#[no_mangle]
pub extern "system" fn Java_org_zerodenet_znetsink_nativeio_NativePluginJobs_nativeRun(
    mut env: JNIEnv<'_>,
    class: JClass<'_>,
    context: JObject<'_>,
    run_id: JString<'_>,
) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let id: String = env.get_string(&run_id).map_err(|_| bridge_error())?.into();
        execute(&mut env, class, context, &id)
    }));
    clear_exception(&mut env);
    let value = match result {
        Ok(Ok(outcome)) => serde_json::to_string(&outcome).unwrap_or_default(),
        _ => "{\"state\":\"failed\",\"reason\":\"native-run-unconfirmed\"}".into(),
    };
    env.new_string(value)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "system" fn Java_org_zerodenet_znetsink_nativeio_NativePluginJobs_nativeStop(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    run_id: JString<'_>,
) {
    if let Ok(id) = env.get_string(&run_id) {
        let id: String = id.into();
        if valid_id(&id) {
            stop(&id);
        }
    }
    clear_exception(&mut env);
}
