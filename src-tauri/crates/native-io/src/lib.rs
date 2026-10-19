//! Host-only native document boundary. There are deliberately no WebView plugin commands.
//! Run blocking methods on a worker thread; system controllers outlive page navigation.
mod policy;
mod vault;
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeLicenseResponse {
    pub status: String,
    pub content: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostEnvironment {
    pub platform: String,
    pub device_class: String,
    #[serde(default)]
    pub secure_vault_api: bool,
}
impl HostEnvironment {
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    fn validated(self, platform: &str) -> Result<Self> {
        if self.platform != platform || !matches!(self.device_class.as_str(), "phone" | "tablet") {
            return Err(Error::InvalidReceipt("invalid_host_environment"));
        }
        Ok(self)
    }
}

impl NativeLicenseResponse {
    pub fn validated(self) -> Result<Self> {
        if !matches!(self.status.as_str(), "available" | "missing" | "invalid")
            || self
                .content
                .as_ref()
                .is_some_and(|value| value.len() > 16 * 1024 * 1024)
            || (self.status != "available" && self.content.is_some())
            || (self.status == "available" && self.content.is_none())
            || self.reason.as_ref().is_some_and(|reason| {
                reason.len() > 96
                    || !reason
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            })
        {
            return Err(Error::InvalidReceipt("invalid_license_asset_response"));
        }
        Ok(self)
    }
}

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
    #[cfg(all(target_os = "ios", debug_assertions))]
    pub fn plugin_vault_master_smoke(
        &self,
        nonce: &str,
        create: bool,
    ) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::InvalidRequest("invalid_smoke_scope"));
        }
        let response: vault::Response = self
            .handle
            .run_mobile_plugin(
                "readPluginVaultMaster",
                &vault::Request {
                    create,
                    smoke_nonce: Some(nonce.into()),
                },
            )
            .map_err(|_| Error::BridgeResultUnknown)?;
        response.take(create)
    }
    #[cfg(all(target_os = "ios", debug_assertions))]
    pub fn plugin_vault_smoke_cleanup(&self, nonce: &str) -> Result<()> {
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::InvalidRequest("invalid_smoke_scope"));
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Cleared {
            status: String,
            reason: Option<String>,
        }
        let response: Cleared = self
            .handle
            .run_mobile_plugin(
                "clearPluginVaultSmoke",
                &vault::Request {
                    create: false,
                    smoke_nonce: Some(nonce.into()),
                },
            )
            .map_err(|_| Error::BridgeResultUnknown)?;
        if response.status == "cleared" && response.reason.is_none() {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    /// Host-only platform vault key; never registered as a WebView command.
    pub fn plugin_vault_master(&self, create: bool) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let response: vault::Response = self
                .handle
                .run_mobile_plugin(
                    "readPluginVaultMaster",
                    &vault::Request {
                        create,
                        smoke_nonce: None,
                    },
                )
                .map_err(|_| Error::BridgeResultUnknown)?;
            response.take(create)
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = create;
            Err(Error::Unavailable)
        }
    }
    /// Read system device facts without a picker or permission prompt.
    pub fn host_environment(&self) -> Result<HostEnvironment> {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let response: HostEnvironment = self
                .handle
                .run_mobile_plugin(
                    "readHostEnvironment",
                    std::collections::BTreeMap::<String, String>::new(),
                )
                .map_err(|_| Error::BridgeResultUnknown)?;
            response.validated(std::env::consts::OS)
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            Err(Error::Unavailable)
        }
    }
    /// Read the fixed packaged license asset. No document picker, path or grant is accepted.
    pub fn license_inventory(&self) -> Result<NativeLicenseResponse> {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let response: NativeLicenseResponse = self
                .handle
                .run_mobile_plugin(
                    "readLicenseInventory",
                    std::collections::BTreeMap::<String, String>::new(),
                )
                .map_err(|_| Error::BridgeResultUnknown)?;
            response.validated()
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            Err(Error::Unavailable)
        }
    }
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

#[cfg(test)]
mod license_response_tests {
    use super::*;
    #[test]
    fn device_environment_rejects_wrong_os_unknown_shape_and_unexpected_fields() {
        for body in [
            r#"{"platform":"ios","deviceClass":"phone"}"#,
            r#"{"platform":"android","deviceClass":"unknown"}"#,
        ] {
            let value: HostEnvironment = serde_json::from_str(body).unwrap();
            assert!(value.validated("android").is_err());
        }
        assert!(serde_json::from_str::<HostEnvironment>(
            r#"{"platform":"android","deviceClass":"phone","override":true}"#
        )
        .is_err());
        for shape in ["phone", "tablet"] {
            assert!(HostEnvironment {
                platform: "android".into(),
                device_class: shape.into(),
                secure_vault_api: false,
            }
            .validated("android")
            .is_ok());
        }
    }
    fn response(
        status: &str,
        content: Option<&str>,
        reason: Option<&str>,
    ) -> NativeLicenseResponse {
        NativeLicenseResponse {
            status: status.into(),
            content: content.map(str::to_owned),
            reason: reason.map(str::to_owned),
        }
    }
    #[test]
    fn fixed_asset_receipt_distinguishes_missing_from_available() {
        assert!(response("available", Some("{}"), None).validated().is_ok());
        assert!(
            response("missing", None, Some("license_asset_not_packaged"))
                .validated()
                .is_ok()
        );
        assert!(response("available", None, None).validated().is_err());
        assert!(response("missing", Some("{}"), None).validated().is_err());
    }
    #[test]
    fn fixed_asset_receipt_rejects_raw_path_errors_and_oversized_bodies() {
        assert!(response("unknown", None, None).validated().is_err());
        assert!(response("invalid", None, Some("/private/provider/token"))
            .validated()
            .is_err());
        assert!(
            response("invalid", None, Some("https://provider/?token=secret"))
                .validated()
                .is_err()
        );
        let oversized = "x".repeat(16 * 1024 * 1024 + 1);
        assert!(response("available", Some(&oversized), None)
            .validated()
            .is_err());
    }
}
