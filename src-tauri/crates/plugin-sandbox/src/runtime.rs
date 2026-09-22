pub use crate::bridge::Summary;
use crate::{
    contract::{Component, Error, Isolation, Target},
    policy::Authority,
};
use rquickjs::{
    loader::{BuiltinLoader, ImportAttributes, Resolver},
    Context, Ctx, Function, Module, Runtime,
};
use std::{
    cell::Cell,
    collections::BTreeSet,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct PackageResolver {
    names: BTreeSet<String>,
    root: String,
}

impl Resolver for PackageResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if attributes.is_some() || !(name.starts_with("./") || name.starts_with("../")) {
            return Err(rquickjs::Error::new_resolving(base, name));
        }
        let mut parts: Vec<&str> = base
            .rsplit_once('/')
            .map(|(parent, _)| parent.split('/').collect())
            .unwrap_or_default();
        for segment in name.split('/') {
            match segment {
                "" | "." => {}
                ".." if !parts.is_empty() => {
                    parts.pop();
                }
                ".." => return Err(rquickjs::Error::new_resolving(base, name)),
                value => parts.push(value),
            }
        }
        let resolved = parts.join("/");
        if resolved.starts_with(&self.root) && self.names.contains(&resolved) {
            Ok(resolved)
        } else {
            Err(rquickjs::Error::new_resolving(base, name))
        }
    }
}

pub type HostSdkDispatcher =
    Arc<dyn Fn(&crate::policy::Lease, &str) -> Result<String, Error> + Send + Sync + 'static>;

pub fn execute(
    component: &Component,
    authority: &Authority,
    selection: Option<(String, Summary)>,
    cancelled: Arc<AtomicBool>,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        selection,
        None,
        cancelled,
        false,
        env!("CARGO_PKG_VERSION"),
        None,
        false,
    )
}

/// Native host supplies its product version; the VM library version is not the client version.
pub fn execute_for_host(
    component: &Component,
    authority: &Authority,
    selection: Option<(String, Summary)>,
    cancelled: Arc<AtomicBool>,
    host_version: &str,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        selection,
        None,
        cancelled,
        true,
        host_version,
        None,
        false,
    )
}

/// Native host entry with a bounded, host-validated declarative input.
pub fn execute_for_host_with_input(
    component: &Component,
    authority: &Authority,
    input: serde_json::Value,
    cancelled: Arc<AtomicBool>,
    host_version: &str,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        None,
        Some(input),
        cancelled,
        true,
        host_version,
        None,
        false,
    )
}

/// Native page invocation with the same permission-checked SDK bridge as a
/// scheduled action, but ordinary execution limits and lease semantics.
pub fn execute_for_host_with_input_and_sdk(
    component: &Component,
    authority: &Authority,
    input: serde_json::Value,
    cancelled: Arc<AtomicBool>,
    host_version: &str,
    dispatcher: HostSdkDispatcher,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        None,
        Some(input),
        cancelled,
        true,
        host_version,
        Some(dispatcher),
        false,
    )
}

/// Scheduled native-host entry. The signed guest remains CPU/output bounded,
/// while `hostSdkCall` delegates typed calls through the host's existing
/// permission and resource enforcement.
pub fn execute_scheduled_for_host_with_input(
    component: &Component,
    authority: &Authority,
    input: serde_json::Value,
    cancelled: Arc<AtomicBool>,
    host_version: &str,
    dispatcher: HostSdkDispatcher,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        None,
        Some(input),
        cancelled,
        true,
        host_version,
        Some(dispatcher),
        true,
    )
}

/// Explicit native test entry; unavailable in default builds and the GUI.
#[cfg(feature = "network-lab")]
pub fn execute_network_lab(
    component: &Component,
    authority: &Authority,
    cancelled: Arc<AtomicBool>,
) -> Result<serde_json::Value, Error> {
    execute_inner(
        component,
        authority,
        None,
        None,
        cancelled,
        true,
        env!("CARGO_PKG_VERSION"),
        None,
        false,
    )
}

fn execute_inner(
    component: &Component,
    authority: &Authority,
    selection: Option<(String, Summary)>,
    input: Option<serde_json::Value>,
    cancelled: Arc<AtomicBool>,
    network_enabled: bool,
    host_version: &str,
    sdk_dispatcher: Option<HostSdkDispatcher>,
    scheduled: bool,
) -> Result<serde_json::Value, Error> {
    component.compatible(&Target::native_desktop()?, host_version, Isolation::Vm)?;
    let lease = Arc::new(if scheduled {
        authority.begin_scheduled(component, cancelled.clone())?
    } else {
        authority.begin_cancelled(component, cancelled.clone())?
    });
    let limits = &component.manifest.limits;
    let cpu_budget = Duration::from_millis(limits.timeout_ms);
    let cpu_elapsed = Rc::new(Cell::new(Duration::ZERO));
    let cpu_resumed = Rc::new(Cell::new(Instant::now()));
    let failure = Rc::new(Cell::new(None));
    let runtime = Runtime::new().map_err(|_| Error::BudgetExceeded)?;
    runtime.set_memory_limit(limits.memory_bytes);
    runtime.set_max_stack_size(limits.stack_bytes);
    if component.module_entry.is_some() {
        let mut loader = BuiltinLoader::default();
        for (path, source) in &component.modules {
            loader.add_module(path.clone(), source.as_bytes());
        }
        runtime.set_loader(
            PackageResolver {
                names: component.modules.keys().cloned().collect(),
                root: format!("components/{}/", component.manifest.component_id),
            },
            loader,
        );
    }
    let check_lease = lease.clone();
    let check_cancel = cancelled.clone();
    let check_failure = failure.clone();
    let check_cpu_elapsed = cpu_elapsed.clone();
    let check_cpu_resumed = cpu_resumed.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let reason = check_lease.check(None).err().or_else(|| {
            if check_cancel.load(Ordering::Relaxed) {
                Some(Error::Cancelled)
            } else if check_cpu_elapsed
                .get()
                .saturating_add(Instant::now().saturating_duration_since(check_cpu_resumed.get()))
                >= cpu_budget
            {
                Some(Error::Deadline)
            } else {
                None
            }
        });
        if let Some(error) = reason {
            check_failure.set(Some(error));
        }
        reason.is_some()
    })));
    // Promise also initializes async function prototypes used by the hardening bootstrap.
    // We never run pending jobs, and Promise results are rejected.
    let context = Context::custom::<(
        rquickjs::context::intrinsic::Eval,
        rquickjs::context::intrinsic::Json,
        rquickjs::context::intrinsic::Promise,
    )>(&runtime)
    .map_err(|_| Error::BudgetExceeded)?;
    let result = context.with(|ctx| -> Result<serde_json::Value, Error> {
        // V2 loads only signed package-relative modules; no std/os, native modules, jobs, or host IO.
        ctx.eval::<(), _>(r#"
            for (const f of [function(){}, function*(){}, async function(){}, async function*(){}]) {
                Object.defineProperty(Object.getPrototypeOf(f), 'constructor', {value: undefined, writable:false, configurable:false});
            }
            for (const key of ['eval','Function']) Object.defineProperty(globalThis,key,{value:undefined,writable:false,configurable:false});
        "#).map_err(|_| Error::GuestException)?;
        let input = serde_json::to_string(&input.unwrap_or(serde_json::Value::Null))
            .map_err(|_| Error::InvalidOutput)?;
        let input_limit = if scheduled { 256 * 1024 } else { 16 * 1024 };
        if input.len() > input_limit {
            return Err(Error::BudgetExceeded);
        }
        ctx.globals()
            .set("__pluginInputJson", input)
            .map_err(|_| Error::BudgetExceeded)?;
        ctx.eval::<(), _>(r#"
            Object.defineProperty(globalThis, 'pluginInput', {
                value: JSON.parse(globalThis.__pluginInputJson),
                writable: false,
                configurable: false,
            });
            delete globalThis.__pluginInputJson;
        "#).map_err(|_| Error::InvalidOutput)?;
        let callback_lease = lease.clone();
        let callback_failure = failure.clone();
        let callback_cancel = cancelled.clone();
        let identity = serde_json::json!({"plugin_id":component.manifest.plugin_id,"component_id":component.manifest.component_id,"digest":component.digest}).to_string();
        let bridge = crate::bridge::Bridge { identity, selection, network_enabled, output_bytes: limits.output_bytes };
        let callback = Function::new(ctx.clone(), move |callback_ctx: rquickjs::Ctx<'_>, input: rquickjs::String<'_>| -> rquickjs::Result<String> {
            let operation = || -> Result<String, Error> {
                if let Some(error) = callback_failure.get() { return Err(error); }
                if callback_cancel.load(Ordering::Relaxed) { return Err(Error::Cancelled); }
                callback_lease.check(None)?;
                let input = input.to_cstring().map_err(|_| Error::InvalidOutput)?;
                if input.len() > 128 * 1024 { return Err(Error::BudgetExceeded); }
                bridge.dispatch(&callback_lease, input.as_str())
            };
            match operation() {
                Ok(value) => Ok(value),
                Err(error) => { callback_failure.set(Some(error)); Err(rquickjs::Exception::throw_message(&callback_ctx, "host capability denied or budget exhausted")) }
            }
        }).map_err(|_| Error::GuestException)?;
        ctx.globals().set("hostCall", callback).map_err(|_| Error::GuestException)?;
        ctx.eval::<(), _>("Object.defineProperty(globalThis,'hostCall',{writable:false,configurable:false});").map_err(|_| Error::GuestException)?;
        if let Some(dispatcher) = sdk_dispatcher.clone() {
            let sdk_lease = lease.clone();
            let sdk_failure = failure.clone();
            let sdk_cancel = cancelled.clone();
            let sdk_cpu_elapsed = cpu_elapsed.clone();
            let sdk_cpu_resumed = cpu_resumed.clone();
            let sdk_calls = Rc::new(Cell::new(0_u32));
            let max_calls = limits.max_calls;
            let callback = Function::new(ctx.clone(), move |callback_ctx: rquickjs::Ctx<'_>, input: rquickjs::String<'_>| -> rquickjs::Result<String> {
                let operation = || -> Result<String, Error> {
                    if let Some(error) = sdk_failure.get() { return Err(error); }
                    if sdk_cancel.load(Ordering::Relaxed) { return Err(Error::Cancelled); }
                    sdk_lease.check(None)?;
                    let next = sdk_calls.get().saturating_add(1);
                    if next > max_calls { return Err(Error::BudgetExceeded); }
                    sdk_calls.set(next);
                    let input = input.to_cstring().map_err(|_| Error::InvalidOutput)?;
                    if input.len() > 8 * 1024 * 1024 { return Err(Error::BudgetExceeded); }
                    let entered = Instant::now();
                    sdk_cpu_elapsed.set(sdk_cpu_elapsed.get().saturating_add(entered.saturating_duration_since(sdk_cpu_resumed.get())));
                    let result = dispatcher(&sdk_lease, input.as_str());
                    sdk_cpu_resumed.set(Instant::now());
                    result
                };
                match operation() {
                    Ok(value) if value.len() <= 8 * 1024 * 1024 => Ok(value),
                    Ok(_) => {
                        sdk_failure.set(Some(Error::BudgetExceeded));
                        Err(rquickjs::Exception::throw_message(&callback_ctx, "host SDK result exceeded budget"))
                    }
                    Err(error) => {
                        sdk_failure.set(Some(error));
                        Err(rquickjs::Exception::throw_message(&callback_ctx, "host SDK capability denied or failed"))
                    }
                }
            }).map_err(|_| Error::GuestException)?;
            ctx.globals().set("hostSdkCall", callback).map_err(|_| Error::GuestException)?;
            ctx.eval::<(), _>("Object.defineProperty(globalThis,'hostSdkCall',{writable:false,configurable:false});").map_err(|_| Error::GuestException)?;
        }
        let value: rquickjs::Value = if let Some(entry) = &component.module_entry {
            let declared = Module::declare(ctx.clone(), entry.as_str(), component.source.as_bytes()).map_err(|_| Error::GuestException)?;
            let (module, promise) = declared.eval().map_err(|_| Error::GuestException)?;
            promise.result::<()>().ok_or(Error::InvalidOutput)?.map_err(|_| Error::GuestException)?;
            let entrypoint: Function = module.get("default").map_err(|_| Error::GuestException)?;
            entrypoint.call(()).map_err(|_| Error::GuestException)?
        } else {
            ctx.eval(component.source.as_bytes()).map_err(|_| Error::GuestException)?
        };
        if value.is_promise() { return Err(Error::InvalidOutput); }
        let json = ctx.json_stringify(value).map_err(|_| Error::InvalidOutput)?.ok_or(Error::InvalidOutput)?;
        let bytes = json.to_cstring().map_err(|_| Error::InvalidOutput)?;
        if bytes.len() > limits.output_bytes { return Err(Error::BudgetExceeded); }
        serde_json::from_str(bytes.as_str()).map_err(|_| Error::InvalidOutput)
    });
    lease.check(None)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let total_cpu = cpu_elapsed
        .get()
        .saturating_add(Instant::now().saturating_duration_since(cpu_resumed.get()));
    if total_cpu >= cpu_budget {
        return Err(Error::Deadline);
    }
    if let Some(error) = failure.get() {
        return Err(error);
    }
    result
}
