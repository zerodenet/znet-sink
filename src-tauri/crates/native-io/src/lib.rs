//! Host-only native document boundary. There are deliberately no WebView plugin commands.
//! Run blocking methods on a worker thread; system controllers outlive page navigation.
mod policy;
use policy::{Action, Request};
pub use policy::{
    Error, ExportOwnership, ExportPurpose, ImportPurpose, PreparedExport, Receipt, ReceiptStatus,
    Result, CONFIGURATION_MAX_BYTES, DIAGNOSTIC_MAX_BYTES, SETTINGS_MAX_BYTES,
};
use std::sync::Mutex;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_znet_native_io);

pub struct NativeIo<R: Runtime> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    runtime: std::marker::PhantomData<fn() -> R>,
    active: Mutex<()>,
}
pub trait NativeIoExt<R: Runtime> {
    fn native_io(&self) -> &NativeIo<R>;
}
impl<R: Runtime, T: Manager<R>> NativeIoExt<R> for T {
    fn native_io(&self) -> &NativeIo<R> {
        self.state::<NativeIo<R>>().inner()
    }
}
impl<R: Runtime> NativeIo<R> {
    /// This reflects a compiled adapter, not device/platform acceptance.
    pub fn source_available(&self) -> bool {
        cfg!(any(target_os = "android", target_os = "ios"))
    }
    pub fn pick_read(
        &self,
        operation_id: &str,
        purpose: ImportPurpose,
        maximum_bytes: u32,
    ) -> Result<Receipt> {
        self.run(
            Request::read(operation_id, purpose, maximum_bytes)?,
            Action::Read,
        )
    }
    pub fn save(&self, operation_id: &str, export: &PreparedExport) -> Result<Receipt> {
        self.run(Request::export(operation_id, export)?, Action::Save)
    }
    pub fn share(&self, operation_id: &str, export: &PreparedExport) -> Result<Receipt> {
        self.run(Request::export(operation_id, export)?, Action::Share)
    }
    fn run(&self, request: Request<'_>, action: Action) -> Result<Receipt> {
        let _active = self.active.try_lock().map_err(|_| Error::Busy)?;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            // Once dispatched a lost bridge receipt is ambiguous. Never replay it automatically.
            let receipt = self
                .handle
                .run_mobile_plugin(action.command(), &request)
                .map_err(|_| Error::BridgeResultUnknown)?;
            request.validate(receipt, action)
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = (request, action);
            Err(Error::Unavailable)
        }
    }
}
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("znet-native-io")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let native_io = NativeIo {
                handle: api
                    .register_android_plugin("org.zerodenet.znetsink.nativeio", "NativeIoPlugin")?,
                active: Mutex::new(()),
            };
            #[cfg(target_os = "ios")]
            let native_io = NativeIo {
                handle: api.register_ios_plugin(init_plugin_znet_native_io)?,
                active: Mutex::new(()),
            };
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let native_io = {
                let _ = api;
                NativeIo {
                    runtime: std::marker::PhantomData::<fn() -> R>,
                    active: Mutex::new(()),
                }
            };
            app.manage(native_io);
            Ok(())
        })
        .build()
}
