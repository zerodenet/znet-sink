//! Business-neutral plugin SDK execution for signed management pages.

use super::*;
use crate::services::common::now_unix_ms;
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hpke::{
    aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256, setup_receiver, setup_sender,
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable,
};
use ring::{aead, hmac};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use znet_plugin_sandbox::{
    contract::{Capability, Request},
    sdk::{Call, Method},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NotificationArgs {
    #[serde(default = "default_notification_kind")]
    kind: String,
    message: String,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    action: Option<NotificationActionArgs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginLogArgs {
    level: crate::models::logs::LogLevel,
    message: String,
    #[serde(default)]
    fields: Option<Value>,
}

impl PluginLogArgs {
    fn validate(&self) -> AppResult<()> {
        if self.message.trim().is_empty() || self.message.len() > 2_048 {
            return Err(AppError::invalid_argument("插件日志消息为空或超过 2 KiB"));
        }
        if self.fields.as_ref().is_some_and(|fields| {
            serde_json::to_vec(fields).map_or(true, |bytes| bytes.len() > 4_096)
        }) {
            return Err(AppError::invalid_argument("插件日志字段超过 4 KiB"));
        }
        Ok(())
    }

    fn trusted_fields(&self, plugin_id: &str, component_id: &str) -> Value {
        json!({"pluginId": plugin_id, "componentId": component_id, "data": self.fields})
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationActionArgs {
    page_id: String,
    route: String,
    #[serde(default)]
    reference: Option<String>,
}

fn default_notification_kind() -> String {
    "info".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageAreaArgs {
    area: namespace::Area,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageGetArgs {
    area: namespace::Area,
    key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoragePutArgs {
    area: namespace::Area,
    key: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageMigrationArgs {
    area: namespace::Area,
    from: u32,
    to: u32,
    values: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchedulePutArgs {
    task_id: String,
    action: String,
    interval_seconds: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScheduleDeleteArgs {
    task_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserOpenArgs {
    url: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CallbackArgs {
    session_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileReadArgs {
    selected_path: String,
    #[serde(default)]
    max_bytes: Option<usize>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileWriteArgs {
    selected_path: String,
    data_base64: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterialArgs {
    data_base64: String,
    #[serde(default = "default_material_purpose")]
    purpose: String,
    #[serde(default)]
    lifetime_ms: Option<u64>,
}
fn default_material_purpose() -> String {
    "material".into()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HandleArgs {
    handle: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretReceiveArgs {
    url: String,
    #[serde(default = "default_post")]
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body_base64: Option<String>,
    #[serde(default)]
    lifetime_ms: Option<u64>,
}
fn default_post() -> String {
    "POST".into()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CryptoArgs {
    operation: String,
    key_handle: String,
    #[serde(default)]
    data_base64: Option<String>,
    #[serde(default)]
    input_handle: Option<String>,
    #[serde(default)]
    nonce_base64: Option<String>,
    #[serde(default)]
    aad_base64: Option<String>,
    #[serde(default)]
    lifetime_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfiguredRequestArgs {
    url: String,
    #[serde(default = "default_post")]
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body_base64: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default = "default_network_route")]
    route: String,
}

fn default_network_route() -> String {
    "direct".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistentSecretArgs {
    key: String,
    #[serde(default)]
    value_base64: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CryptoKeyArgs {
    key_name: String,
    #[serde(default)]
    data_base64: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CryptoVerifyArgs {
    public_key_base64: String,
    data_base64: String,
    signature_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CryptoDigestArgs {
    data_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HpkeSealArgs {
    recipient_public_key_base64: String,
    info_base64: String,
    aad_base64: String,
    plaintext_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HpkeOpenArgs {
    private_key_handle: String,
    sender_public_key_base64: String,
    info_base64: String,
    aad_base64: String,
    encapsulation_base64: String,
    ciphertext_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubscriptionApplyArgs {
    provider_id: String,
    remote_subscription_id: String,
    source_name: String,
    subscription_name: String,
    content: String,
    format: String,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubscriptionRemoveArgs {
    subscription_id: String,
    #[serde(default)]
    remove_associated_config: bool,
}

pub(crate) struct PreparedManagedSubscription {
    input: crate::models::subscription::ManagedSubscriptionApply,
    authorization: ManagedSubscriptionAuthorization,
}

impl PreparedManagedSubscription {
    pub(crate) fn into_parts(
        self,
    ) -> (
        crate::models::subscription::ManagedSubscriptionApply,
        ManagedSubscriptionAuthorization,
    ) {
        (self.input, self.authorization)
    }
}

pub(crate) struct ManagedSubscriptionAuthorization {
    lease: znet_plugin_sandbox::policy::Lease,
    request: Request,
}

pub(crate) struct PreparedManagedSubscriptionRemoval {
    pub(crate) subscription_id: String,
    pub(crate) remove_associated_config: bool,
    pub(crate) authorization: ManagedSubscriptionAuthorization,
}

impl ManagedSubscriptionAuthorization {
    pub(crate) fn check(&self) -> AppResult<()> {
        self.lease.check(Some(&self.request)).map_err(io::failure)
    }
}

type HpkeKem = X25519HkdfSha256;
type HpkeAead = ChaCha20Poly1305;
type HpkeKdf = HkdfSha256;

impl Host {
    fn write_plugin_log(
        &self,
        state: &crate::state::app_state::AppState,
        plugin_id: &str,
        component_id: &str,
        arguments: Value,
    ) -> AppResult<()> {
        use crate::models::logs::LogSource;
        let args: PluginLogArgs = decode(arguments)?;
        args.validate()?;
        let now = now_unix_ms();
        let owner = format!("{plugin_id}/{component_id}");
        {
            let mut limits = self.log_times.lock().unwrap();
            let times = limits.entry(owner).or_default();
            while times
                .front()
                .is_some_and(|time| now.saturating_sub(*time) >= 60_000)
            {
                times.pop_front();
            }
            if times.len() >= 120 {
                return Err(AppError {
                    code: "plugin_log_rate_limited",
                    message: "插件日志写入过于频繁，请稍后重试".into(),
                    details: Some(json!({"retryAfterMs": 60_000})),
                });
            }
            times.push_back(now);
        }
        // Identity is host-derived; plugin-provided fields remain nested and cannot
        // impersonate another component or a client/kernel log source.
        let fields = args.trusted_fields(plugin_id, component_id);
        crate::services::logs::append_entry(
            state,
            LogSource::Plugin,
            args.level,
            args.message.trim().to_owned(),
            Some(fields),
        )?;
        Ok(())
    }

    pub(crate) fn managed_subscription_input_with_lease(
        &self,
        plugin_id: &str,
        lease: &znet_plugin_sandbox::policy::Lease,
        call: Call,
    ) -> AppResult<(
        crate::models::subscription::ManagedSubscriptionApply,
        Request,
    )> {
        if !call.validate()
            || call.method != Method::SubscriptionApply
            || call.request.capability != Capability::SubscriptionsManage
        {
            return Err(AppError::invalid_argument("插件托管订阅请求无效"));
        }
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let request = call.request;
        let args: SubscriptionApplyArgs = decode(call.arguments)?;
        Ok((
            crate::models::subscription::ManagedSubscriptionApply {
                plugin_id: plugin_id.to_owned(),
                provider_id: args.provider_id,
                remote_subscription_id: args.remote_subscription_id,
                source_name: args.source_name,
                subscription_name: args.subscription_name,
                content: args.content,
                format: args.format,
                revision: args.revision,
            },
            request,
        ))
    }

    pub(crate) fn managed_subscription_removal_with_lease(
        &self,
        lease: &znet_plugin_sandbox::policy::Lease,
        call: Call,
    ) -> AppResult<(String, bool, Request)> {
        if !call.validate()
            || call.method != Method::SubscriptionRemove
            || call.request.capability != Capability::SubscriptionsManage
        {
            return Err(AppError::invalid_argument("插件托管订阅移除请求无效"));
        }
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let request = call.request;
        let args: SubscriptionRemoveArgs = decode(call.arguments)?;
        Ok((args.subscription_id, args.remove_associated_config, request))
    }

    pub(crate) fn prepare_managed_subscription(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        call: Call,
    ) -> AppResult<PreparedManagedSubscription> {
        if !call.validate()
            || call.method != Method::SubscriptionApply
            || call.request.capability != Capability::SubscriptionsManage
        {
            return Err(AppError::invalid_argument("插件托管订阅请求无效"));
        }
        let (component, authority) =
            self.sdk_context(manager, plugin_id, component_id, &call.request)?;
        let lease = authority
            .begin_host_call(
                &component,
                Duration::from_millis(call.budget.timeout_ms),
                call.budget.max_result_bytes,
            )
            .map_err(io::failure)?;
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let args: SubscriptionApplyArgs = decode(call.arguments)?;
        Ok(PreparedManagedSubscription {
            input: crate::models::subscription::ManagedSubscriptionApply {
                plugin_id: plugin_id.to_owned(),
                provider_id: args.provider_id,
                remote_subscription_id: args.remote_subscription_id,
                source_name: args.source_name,
                subscription_name: args.subscription_name,
                content: args.content,
                format: args.format,
                revision: args.revision,
            },
            authorization: ManagedSubscriptionAuthorization {
                lease,
                request: call.request,
            },
        })
    }

    pub(crate) fn prepare_managed_subscription_removal(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        call: Call,
    ) -> AppResult<PreparedManagedSubscriptionRemoval> {
        if !call.validate()
            || call.method != Method::SubscriptionRemove
            || call.request.capability != Capability::SubscriptionsManage
        {
            return Err(AppError::invalid_argument("插件托管订阅移除请求无效"));
        }
        let (component, authority) =
            self.sdk_context(manager, plugin_id, component_id, &call.request)?;
        let lease = authority
            .begin_host_call(
                &component,
                Duration::from_millis(call.budget.timeout_ms),
                call.budget.max_result_bytes,
            )
            .map_err(io::failure)?;
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let args: SubscriptionRemoveArgs = decode(call.arguments)?;
        Ok(PreparedManagedSubscriptionRemoval {
            subscription_id: args.subscription_id,
            remove_associated_config: args.remove_associated_config,
            authorization: ManagedSubscriptionAuthorization {
                lease,
                request: call.request,
            },
        })
    }

    pub(super) fn configured_origin(
        &self,
        plugin_id: &str,
        component_id: &str,
        field: &str,
        request_url: &str,
    ) -> AppResult<String> {
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        let schema = loaded
            .component
            .manifest()
            .configuration
            .as_ref()
            .ok_or_else(|| AppError::invalid_argument("插件没有声明配置模式"))?;
        let mut values = schema.defaults();
        values.extend(loaded.configuration.clone());
        schema
            .validate_values(&values)
            .map_err(|_| AppError::invalid_argument("插件来源配置尚未完成"))?;
        let value = values
            .get(field)
            .ok_or_else(|| AppError::invalid_argument("声明的来源配置字段不存在"))?;
        let configuration_field = schema
            .fields
            .iter()
            .find(|candidate| candidate.id == field)
            .ok_or_else(|| AppError::invalid_argument("声明的来源配置字段不存在"))?;
        let requested_origin =
            znet_client_capabilities::network::origin(request_url).map_err(io::failure)?;
        let allowed = configuration_field
            .https_origins(value)
            .ok_or_else(|| AppError::invalid_argument("已配置来源必须是精确的 HTTPS origin"))?;
        allowed
            .into_iter()
            .find(|origin| origin == &requested_origin)
            .ok_or_else(|| AppError::invalid_argument("请求地址不在插件已配置的 HTTPS 来源中"))
    }

    fn sdk_context(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        request: &Request,
    ) -> AppResult<(
        Arc<znet_plugin_sandbox::contract::Component>,
        znet_plugin_sandbox::policy::Authority,
    )> {
        self.checked_rescan(manager)?;
        let key = format!("{plugin_id}/{component_id}");
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        let manifest = loaded.component.manifest();
        if manifest.plugin_id != plugin_id
            || manifest.component_id != component_id
            || loaded.blocked.is_some()
            || !loaded.consent.enabled
            || !loaded.consent.grants.contains(request)
            || !manifest
                .required
                .iter()
                .chain(&manifest.optional)
                .any(|declared| declared == request)
        {
            return Err(AppError::invalid_argument(
                "插件未启用，或此 SDK 操作没有获得对应权限",
            ));
        }
        Ok((
            Arc::clone(&loaded.component),
            loaded.authority.clone().ok_or_else(stale)?,
        ))
    }

    pub fn sdk_call(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        call: Call,
    ) -> AppResult<Value> {
        self.sdk_call_with_network_proxy(manager, plugin_id, component_id, call, None, None)
    }

    pub(crate) fn sdk_call_with_network_proxy(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        call: Call,
        network_proxy: Option<String>,
        log_state: Option<&crate::state::app_state::AppState>,
    ) -> AppResult<Value> {
        if !call.validate() || !method_matches(call.method, call.request.capability) {
            return Err(AppError::invalid_argument("插件 SDK 请求格式或能力不匹配"));
        }
        let budget = call.budget;
        let (component, authority) =
            self.sdk_context(manager, plugin_id, component_id, &call.request)?;
        let lease = authority
            .begin_host_call(
                &component,
                Duration::from_millis(budget.timeout_ms),
                budget.max_result_bytes,
            )
            .map_err(io::failure)?;
        self.sdk_call_with_lease(
            manager,
            plugin_id,
            component_id,
            &lease,
            call,
            network_proxy,
            log_state,
        )
    }

    pub(crate) fn sdk_call_with_lease(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        lease: &znet_plugin_sandbox::policy::Lease,
        call: Call,
        network_proxy: Option<String>,
        log_state: Option<&crate::state::app_state::AppState>,
    ) -> AppResult<Value> {
        if !call.validate() || !method_matches(call.method, call.request.capability) {
            return Err(AppError::invalid_argument("插件 SDK 请求格式或能力不匹配"));
        }
        let budget = call.budget;
        let started = Instant::now();
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let value = match call.method {
            Method::LogWrite => {
                let state =
                    log_state.ok_or_else(|| AppError::invalid_argument("插件日志宿主不可用"))?;
                self.write_plugin_log(state, plugin_id, component_id, call.arguments)?;
                Value::Bool(true)
            }
            Method::StorageGet => {
                let args: StorageGetArgs = decode(call.arguments)?;
                serde_json::to_value(self.storage_get(plugin_id, args.area, &args.key)?)
                    .map_err(io::failure)?
            }
            Method::StoragePut => {
                let args: StoragePutArgs = decode(call.arguments)?;
                self.storage_put(plugin_id, args.area, args.key, args.value)?;
                Value::Bool(true)
            }
            Method::StorageDelete => {
                let args: StorageGetArgs = decode(call.arguments)?;
                self.storage_delete(plugin_id, args.area, &args.key)?;
                Value::Bool(true)
            }
            Method::StorageList => {
                let args: StorageAreaArgs = decode(call.arguments)?;
                json!({"keys": self.storage_list(plugin_id, args.area)?})
            }
            Method::StorageExport => {
                let args: StorageAreaArgs = decode(call.arguments)?;
                let (schema_version, values) = self.storage_export(plugin_id, args.area)?;
                json!({"schemaVersion":schema_version,"values":values})
            }
            Method::StorageClear => {
                let args: StorageAreaArgs = decode(call.arguments)?;
                self.storage_clear(plugin_id, args.area)?;
                Value::Bool(true)
            }
            Method::StorageMigrate => {
                let args: StorageMigrationArgs = decode(call.arguments)?;
                self.storage_migrate(plugin_id, args.area, args.from, args.to, args.values)?;
                Value::Bool(true)
            }
            Method::NotificationPost => {
                self.notification(plugin_id, component_id, call.arguments)?
            }
            Method::SchedulePut => {
                let args: SchedulePutArgs = decode(call.arguments)?;
                serde_json::to_value(self.schedule_put(
                    plugin_id,
                    component_id,
                    args.task_id,
                    args.action,
                    args.interval_seconds,
                )?)
                .map_err(io::failure)?
            }
            Method::ScheduleList => {
                serde_json::to_value(self.schedule_list(plugin_id, component_id)?)
                    .map_err(io::failure)?
            }
            Method::ScheduleDelete => {
                let args: ScheduleDeleteArgs = decode(call.arguments)?;
                self.schedule_delete(plugin_id, component_id, &args.task_id)?;
                Value::Bool(true)
            }
            Method::BrowserOpen => {
                let args: BrowserOpenArgs = decode(call.arguments)?;
                let origin =
                    znet_client_capabilities::network::origin(&args.url).map_err(io::failure)?;
                if origin != call.request.scope {
                    return Err(AppError::invalid_argument("浏览器地址超出已授权来源"));
                }
                json!({"url":args.url})
            }
            Method::CallbackCreate => serde_json::to_value(
                self.callbacks
                    .create(format!("{plugin_id}/{component_id}"))?,
            )
            .map_err(io::failure)?,
            Method::CallbackPoll => {
                let args: CallbackArgs = decode(call.arguments)?;
                self.callbacks
                    .poll(&format!("{plugin_id}/{component_id}"), &args.session_id)?
            }
            Method::CallbackCancel => {
                let args: CallbackArgs = decode(call.arguments)?;
                self.callbacks
                    .cancel(&format!("{plugin_id}/{component_id}"), &args.session_id)?;
                Value::Bool(true)
            }
            Method::FileRead => {
                let args: FileReadArgs = decode(call.arguments)?;
                let maximum = args
                    .max_bytes
                    .unwrap_or(4 * 1024 * 1024)
                    .min(4 * 1024 * 1024);
                let selected = znet_client_capabilities::files::Selection::from_user_path(
                    PathBuf::from(&args.selected_path),
                );
                let host_lease = znet_client_capabilities::host::operation(
                    manager,
                    &format!("plugin.file.read:{plugin_id}/{component_id}"),
                    selected.permission(),
                    1,
                    maximum,
                    Duration::from_millis(budget.timeout_ms),
                )
                .map_err(io::failure)?;
                let bytes = znet_client_capabilities::files::read(&host_lease, &selected, maximum)
                    .and_then(|resource| resource.take(&host_lease))
                    .map_err(io::failure)?;
                json!({
                    "name": PathBuf::from(args.selected_path).file_name().and_then(|value| value.to_str()).unwrap_or("selected-file"),
                    "dataBase64": STANDARD.encode(bytes),
                })
            }
            Method::FileWrite => {
                let args: FileWriteArgs = decode(call.arguments)?;
                let bytes = STANDARD
                    .decode(&args.data_base64)
                    .map_err(|_| AppError::invalid_argument("插件文件内容不是有效的 Base64"))?;
                if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
                    return Err(AppError::invalid_argument("插件文件内容为空或超过 4 MiB"));
                }
                let publication = znet_client_capabilities::files::Publication::from_host_path(
                    PathBuf::from(&args.selected_path),
                );
                let host_lease = znet_client_capabilities::host::operation(
                    manager,
                    &format!("plugin.file.write:{plugin_id}/{component_id}"),
                    publication.permission(),
                    1,
                    bytes.len(),
                    Duration::from_millis(budget.timeout_ms),
                )
                .map_err(io::failure)?;
                znet_client_capabilities::files::publish(
                    &host_lease,
                    &publication,
                    &bytes,
                    4 * 1024 * 1024,
                )
                .and_then(|resource| resource.take(&host_lease))
                .map_err(io::failure)?;
                json!({"bytesWritten":bytes.len()})
            }
            Method::MaterialSubmit => {
                let args: MaterialArgs = decode(call.arguments)?;
                let bytes = STANDARD
                    .decode(args.data_base64)
                    .map_err(|_| AppError::invalid_argument("插件材料不是有效的 Base64"))?;
                let (handle, expires_at_unix_ms) = self.sensitive.insert(
                    format!("{plugin_id}/{component_id}"),
                    args.purpose,
                    bytes,
                    args.lifetime_ms.unwrap_or(10 * 60 * 1000),
                )?;
                json!({"handle":handle,"expiresAtUnixMs":expires_at_unix_ms})
            }
            Method::MaterialDrop => {
                let args: HandleArgs = decode(call.arguments)?;
                self.sensitive
                    .remove(&format!("{plugin_id}/{component_id}"), &args.handle)?;
                Value::Bool(true)
            }
            Method::SecretReceive => {
                let args: SecretReceiveArgs = decode(call.arguments)?;
                let origin =
                    znet_client_capabilities::network::origin(&args.url).map_err(io::failure)?;
                if origin != call.request.scope {
                    return Err(AppError::invalid_argument("敏感响应地址超出已授权来源"));
                }
                let body = args
                    .body_base64
                    .map(|value| {
                        STANDARD
                            .decode(value)
                            .map_err(|_| AppError::invalid_argument("请求正文不是有效的 Base64"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                let host_lease = znet_client_capabilities::host::operation(
                    manager,
                    &format!("plugin.secret.receive:{plugin_id}/{component_id}"),
                    Permission::new("network.request", &origin),
                    1,
                    8 * 1024 * 1024,
                    Duration::from_millis(budget.timeout_ms),
                )
                .map_err(io::failure)?;
                let response = znet_client_capabilities::network::request(
                    &host_lease,
                    &znet_client_capabilities::network::Request {
                        method: args.method,
                        url: args.url,
                        headers: args.headers,
                        body,
                    },
                    8 * 1024 * 1024,
                )
                .and_then(|resource| resource.take(&host_lease))
                .map_err(io::failure)?;
                if !(200..300).contains(&response.status) {
                    return Err(AppError {
                        code: "plugin_secret_transport",
                        message: format!("敏感响应端点返回 HTTP {}", response.status),
                        details: None,
                    });
                }
                let content_type = response.headers.get("content-type").cloned();
                let (handle, expires_at_unix_ms) = self.sensitive.insert(
                    format!("{plugin_id}/{component_id}"),
                    "secret".into(),
                    response.body,
                    args.lifetime_ms.unwrap_or(10 * 60 * 1000),
                )?;
                json!({"handle":handle,"expiresAtUnixMs":expires_at_unix_ms,"contentType":content_type})
            }
            Method::ConfiguredRequest => {
                let args: ConfiguredRequestArgs = decode(call.arguments)?;
                let origin = self.configured_origin(
                    plugin_id,
                    component_id,
                    &call.request.scope,
                    &args.url,
                )?;
                let body = match (args.body, args.body_base64) {
                    (Some(value), None) => value.into_bytes(),
                    (None, Some(value)) => STANDARD
                        .decode(value)
                        .map_err(|_| AppError::invalid_argument("请求正文不是有效的 Base64"))?,
                    (None, None) => Vec::new(),
                    (Some(_), Some(_)) => {
                        return Err(AppError::invalid_argument("请求正文只能使用一种编码"))
                    }
                };
                let route = match args.route.as_str() {
                    "direct" => znet_client_capabilities::network::Route::Direct,
                    "system" => znet_client_capabilities::network::Route::System,
                    "core" => znet_client_capabilities::network::Route::Proxy(
                        network_proxy.clone().ok_or_else(|| {
                            AppError::invalid_argument("客户端代理内核端点不可用")
                        })?,
                    ),
                    _ => return Err(AppError::invalid_argument("插件网络路由无效")),
                };
                let host_lease = znet_client_capabilities::host::operation(
                    manager,
                    &format!("plugin.network.configured:{plugin_id}/{component_id}"),
                    Permission::new("network.configured.request", &call.request.scope),
                    1,
                    8 * 1024 * 1024,
                    Duration::from_millis(budget.timeout_ms),
                )
                .map_err(io::failure)?;
                let response = znet_client_capabilities::network::configured_request(
                    &host_lease,
                    &znet_client_capabilities::network::Request {
                        method: args.method,
                        url: args.url,
                        headers: args.headers,
                        body,
                    },
                    &call.request.scope,
                    &origin,
                    route,
                    8 * 1024 * 1024,
                )
                .and_then(|resource| resource.take(&host_lease))
                .map_err(io::failure)?;
                let body = String::from_utf8(response.body)
                    .map_err(|_| AppError::invalid_argument("插件网络响应不是 UTF-8 文本"))?;
                json!({
                    "status": response.status,
                    "headers": response.headers,
                    "body": body,
                })
            }
            Method::PersistentSecretGet => {
                let _operation = self.operation.lock().unwrap();
                let args: PersistentSecretArgs = decode(call.arguments)?;
                match self.vault_get(plugin_id, &args.key)? {
                    Some(value) => json!({"valueBase64":STANDARD.encode(value.as_slice())}),
                    None => Value::Null,
                }
            }
            Method::PersistentSecretPut => {
                let _operation = self.operation.lock().unwrap();
                let args: PersistentSecretArgs = decode(call.arguments)?;
                let value = args
                    .value_base64
                    .ok_or_else(|| AppError::invalid_argument("缺少凭据内容"))?;
                let value = STANDARD
                    .decode(value)
                    .map_err(|_| AppError::invalid_argument("凭据内容不是有效的 Base64"))?;
                self.vault_put(plugin_id, &args.key, &value)?;
                Value::Bool(true)
            }
            Method::PersistentSecretDelete => {
                let _operation = self.operation.lock().unwrap();
                let args: PersistentSecretArgs = decode(call.arguments)?;
                Value::Bool(self.vault_delete(plugin_id, &args.key)?)
            }
            Method::CryptoKeyGenerate => {
                let _operation = self.operation.lock().unwrap();
                let args: CryptoKeyArgs = decode(call.arguments)?;
                let vault_key = format!("keys/{}", args.key_name);
                let seed = match self.vault_get(plugin_id, &vault_key)? {
                    Some(value) => value,
                    None => {
                        let mut seed = vec![0u8; 32];
                        getrandom::fill(&mut seed).map_err(io::failure)?;
                        self.vault_put(plugin_id, &vault_key, &seed)?;
                        zeroize::Zeroizing::new(seed)
                    }
                };
                let seed: [u8; 32] = seed
                    .as_slice()
                    .try_into()
                    .map_err(|_| AppError::invalid_argument("设备签名密钥无效"))?;
                let signing = SigningKey::from_bytes(&seed);
                json!({"algorithm":"ed25519","publicKeyBase64":STANDARD.encode(signing.verifying_key().as_bytes())})
            }
            Method::CryptoSign => {
                let _operation = self.operation.lock().unwrap();
                let args: CryptoKeyArgs = decode(call.arguments)?;
                let data = STANDARD
                    .decode(
                        args.data_base64
                            .ok_or_else(|| AppError::invalid_argument("缺少待签名数据"))?,
                    )
                    .map_err(|_| AppError::invalid_argument("待签名数据不是有效的 Base64"))?;
                let seed = self
                    .vault_get(plugin_id, &format!("keys/{}", args.key_name))?
                    .ok_or_else(|| {
                        AppError::not_found("plugin_device_key", args.key_name.clone())
                    })?;
                let seed: [u8; 32] = seed
                    .as_slice()
                    .try_into()
                    .map_err(|_| AppError::invalid_argument("设备签名密钥无效"))?;
                json!({"signatureBase64":STANDARD.encode(SigningKey::from_bytes(&seed).sign(&data).to_bytes())})
            }
            Method::CryptoVerify => {
                let args: CryptoVerifyArgs = decode(call.arguments)?;
                let public: [u8; 32] = STANDARD
                    .decode(args.public_key_base64)
                    .map_err(|_| AppError::invalid_argument("签名公钥无效"))?
                    .try_into()
                    .map_err(|_| AppError::invalid_argument("签名公钥无效"))?;
                let data = STANDARD
                    .decode(args.data_base64)
                    .map_err(|_| AppError::invalid_argument("验签数据无效"))?;
                let signature: [u8; 64] = STANDARD
                    .decode(args.signature_base64)
                    .map_err(|_| AppError::invalid_argument("签名无效"))?
                    .try_into()
                    .map_err(|_| AppError::invalid_argument("签名无效"))?;
                let valid = VerifyingKey::from_bytes(&public).is_ok_and(|key| {
                    key.verify(&data, &Signature::from_bytes(&signature))
                        .is_ok()
                });
                json!({"valid":valid})
            }
            Method::CryptoDigest => {
                let args: CryptoDigestArgs = decode(call.arguments)?;
                let data = STANDARD
                    .decode(args.data_base64)
                    .map_err(|_| AppError::invalid_argument("摘要数据无效"))?;
                json!({"digestBase64":STANDARD.encode(Sha256::digest(data))})
            }
            Method::CryptoHpkeKeyGenerate => {
                let (private, public) = HpkeKem::gen_keypair();
                let private_bytes = private.to_bytes().to_vec();
                let (handle, expires_at_unix_ms) = self.sensitive.insert(
                    format!("{plugin_id}/{component_id}"),
                    "hpke-private".into(),
                    private_bytes,
                    10 * 60 * 1000,
                )?;
                json!({
                    "privateKeyHandle":handle,
                    "publicKeyBase64":STANDARD.encode(public.to_bytes()),
                    "expiresAtUnixMs":expires_at_unix_ms,
                })
            }
            Method::CryptoHpkeSeal => {
                let args: HpkeSealArgs = decode(call.arguments)?;
                let recipient = <<HpkeKem as KemTrait>::PublicKey as Deserializable>::from_bytes(
                    &STANDARD
                        .decode(args.recipient_public_key_base64)
                        .map_err(|_| AppError::invalid_argument("HPKE 公钥无效"))?,
                )
                .map_err(|_| AppError::invalid_argument("HPKE 公钥无效"))?;
                let info = STANDARD
                    .decode(args.info_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE info 无效"))?;
                let aad = STANDARD
                    .decode(args.aad_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE AAD 无效"))?;
                let plaintext = STANDARD
                    .decode(args.plaintext_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE 明文无效"))?;
                let (encapsulation, mut context) =
                    setup_sender::<HpkeAead, HpkeKdf, HpkeKem>(&OpModeS::Base, &recipient, &info)
                        .map_err(|_| AppError::invalid_argument("HPKE 请求上下文无效"))?;
                let ciphertext = context
                    .seal(&plaintext, &aad)
                    .map_err(|_| AppError::invalid_argument("HPKE 请求加密失败"))?;
                json!({"encapsulationBase64":STANDARD.encode(encapsulation.to_bytes()),"ciphertextBase64":STANDARD.encode(ciphertext)})
            }
            Method::CryptoHpkeOpen => {
                let args: HpkeOpenArgs = decode(call.arguments)?;
                let owner = format!("{plugin_id}/{component_id}");
                let private = self.sensitive.clone_bytes(
                    &owner,
                    &args.private_key_handle,
                    Some("hpke-private"),
                )?;
                let private =
                    <<HpkeKem as KemTrait>::PrivateKey as Deserializable>::from_bytes(&private)
                        .map_err(|_| AppError::invalid_argument("HPKE 临时私钥无效"))?;
                let sender = <<HpkeKem as KemTrait>::PublicKey as Deserializable>::from_bytes(
                    &STANDARD
                        .decode(args.sender_public_key_base64)
                        .map_err(|_| AppError::invalid_argument("HPKE 发送方公钥无效"))?,
                )
                .map_err(|_| AppError::invalid_argument("HPKE 发送方公钥无效"))?;
                let encapsulation =
                    <<HpkeKem as KemTrait>::EncappedKey as Deserializable>::from_bytes(
                        &STANDARD
                            .decode(args.encapsulation_base64)
                            .map_err(|_| AppError::invalid_argument("HPKE 封装密钥无效"))?,
                    )
                    .map_err(|_| AppError::invalid_argument("HPKE 封装密钥无效"))?;
                let info = STANDARD
                    .decode(args.info_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE info 无效"))?;
                let aad = STANDARD
                    .decode(args.aad_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE AAD 无效"))?;
                let ciphertext = STANDARD
                    .decode(args.ciphertext_base64)
                    .map_err(|_| AppError::invalid_argument("HPKE 密文无效"))?;
                let mut context = setup_receiver::<HpkeAead, HpkeKdf, HpkeKem>(
                    &OpModeR::Auth(sender),
                    &private,
                    &encapsulation,
                    &info,
                )
                .map_err(|_| AppError::invalid_argument("HPKE 响应上下文无效"))?;
                let plaintext = context
                    .open(&ciphertext, &aad)
                    .map_err(|_| AppError::invalid_argument("HPKE 响应校验或解密失败"))?;
                json!({"plaintextBase64":STANDARD.encode(plaintext)})
            }
            Method::CryptoUse => {
                let args: CryptoArgs = decode(call.arguments)?;
                let owner = format!("{plugin_id}/{component_id}");
                let key = self
                    .sensitive
                    .clone_bytes(&owner, &args.key_handle, Some("secret"))?;
                match args.operation.as_str() {
                    "hmac_sha256" => {
                        let data = args
                            .data_base64
                            .ok_or_else(|| AppError::invalid_argument("HMAC 操作缺少数据"))?;
                        let data = STANDARD.decode(data).map_err(|_| {
                            AppError::invalid_argument("HMAC 数据不是有效的 Base64")
                        })?;
                        let key = hmac::Key::new(hmac::HMAC_SHA256, &key);
                        json!({"tagBase64":STANDARD.encode(hmac::sign(&key, &data).as_ref())})
                    }
                    "chacha20poly1305_decrypt" => {
                        let nonce = STANDARD
                            .decode(
                                args.nonce_base64.ok_or_else(|| {
                                    AppError::invalid_argument("解密操作缺少 nonce")
                                })?,
                            )
                            .map_err(|_| AppError::invalid_argument("nonce 不是有效的 Base64"))?;
                        let nonce: [u8; 12] = nonce.try_into().map_err(|_| {
                            AppError::invalid_argument("ChaCha20-Poly1305 nonce 必须是 12 字节")
                        })?;
                        let mut input = match (args.input_handle, args.data_base64) {
                            (Some(handle), None) => {
                                self.sensitive.clone_bytes(&owner, &handle, None)?.to_vec()
                            }
                            (None, Some(value)) => STANDARD
                                .decode(value)
                                .map_err(|_| AppError::invalid_argument("密文不是有效的 Base64"))?,
                            _ => {
                                return Err(AppError::invalid_argument(
                                    "解密操作必须提供一个密文来源",
                                ))
                            }
                        };
                        let aad = args
                            .aad_base64
                            .map(|value| {
                                STANDARD.decode(value).map_err(|_| {
                                    AppError::invalid_argument("AAD 不是有效的 Base64")
                                })
                            })
                            .transpose()?
                            .unwrap_or_default();
                        let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key).map_err(
                            |_| AppError::invalid_argument("ChaCha20-Poly1305 密钥必须是 32 字节"),
                        )?;
                        let key = aead::LessSafeKey::new(key);
                        let output = key
                            .open_in_place(
                                aead::Nonce::assume_unique_for_key(nonce),
                                aead::Aad::from(aad),
                                &mut input,
                            )
                            .map_err(|_| AppError::invalid_argument("受保护材料校验或解密失败"))?
                            .to_vec();
                        let (handle, expires_at_unix_ms) = self.sensitive.insert(
                            owner,
                            "material".into(),
                            output,
                            args.lifetime_ms.unwrap_or(10 * 60 * 1000),
                        )?;
                        json!({"handle":handle,"expiresAtUnixMs":expires_at_unix_ms})
                    }
                    _ => {
                        return Err(AppError::invalid_argument(
                            "客户端不支持请求的通用密码学操作",
                        ))
                    }
                }
            }
            _ => {
                return Err(AppError::invalid_argument(
                    "此 SDK 操作尚未连接到宿主执行器",
                ))
            }
        };
        if started.elapsed() > Duration::from_millis(budget.timeout_ms) {
            return Err(AppError {
                code: "plugin_sdk_deadline",
                message: "插件 SDK 操作超过声明的时间预算".into(),
                details: None,
            });
        }
        if serde_json::to_vec(&value).map_err(io::failure)?.len() > budget.max_result_bytes {
            return Err(AppError {
                code: "plugin_sdk_result_budget",
                message: "插件 SDK 操作结果超过声明的大小预算".into(),
                details: None,
            });
        }
        lease.check(Some(&call.request)).map_err(io::failure)?;
        Ok(value)
    }

    pub fn take_protected_material(
        &self,
        manager: &Manager,
        plugin_id: &str,
        component_id: &str,
        call: Call,
    ) -> AppResult<(
        znet_plugin_sandbox::policy::Lease,
        zeroize::Zeroizing<Vec<u8>>,
    )> {
        if !call.validate()
            || call.method != Method::ProtectedLoad
            || call.request.capability != Capability::RuntimeProtectedLoad
        {
            return Err(AppError::invalid_argument("受保护加载 SDK 请求无效"));
        }
        let args: HandleArgs = decode(call.arguments)?;
        let (component, authority) =
            self.sdk_context(manager, plugin_id, component_id, &call.request)?;
        let lease = authority
            .begin_host_call(
                &component,
                Duration::from_millis(call.budget.timeout_ms),
                call.budget.max_result_bytes,
            )
            .map_err(io::failure)?;
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let bytes = self.sensitive.take(
            &format!("{plugin_id}/{component_id}"),
            &args.handle,
            Some("material"),
        )?;
        Ok((lease, bytes))
    }

    fn notification(
        &self,
        plugin_id: &str,
        component_id: &str,
        arguments: Value,
    ) -> AppResult<Value> {
        let args: NotificationArgs = decode(arguments)?;
        let message = args.message.trim();
        if message.is_empty()
            || message.chars().count() > 240
            || !matches!(args.kind.as_str(), "info" | "success" | "warning" | "error")
        {
            return Err(AppError::invalid_argument("插件通知内容或类型无效"));
        }
        let duration_ms = args.duration_ms.unwrap_or(5_000).clamp(2_000, 15_000);
        let action = if let Some(action) = args.action {
            let safe = |value: &str, maximum: usize| {
                !value.is_empty()
                    && value.len() <= maximum
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            };
            if !safe(&action.page_id, 80)
                || !safe(&action.route, 80)
                || action
                    .reference
                    .as_deref()
                    .is_some_and(|value| !safe(value, 160))
                || !self
                    .state
                    .lock()
                    .unwrap()
                    .pages
                    .get(plugin_id)
                    .is_some_and(|pages| pages.contains_key(&action.page_id))
            {
                return Err(AppError::invalid_argument("插件通知的内部跳转目标无效"));
            }
            Some(json!({
                "pageId": action.page_id,
                "route": action.route,
                "reference": action.reference,
            }))
        } else {
            None
        };
        let now = now_unix_ms();
        let key = format!("{plugin_id}/{component_id}");
        let mut notifications = self.notification_times.lock().unwrap();
        let times = notifications.entry(key).or_default();
        while times
            .front()
            .is_some_and(|value| now.saturating_sub(*value) > 3_600_000)
        {
            times.pop_front();
        }
        let recent = times
            .iter()
            .filter(|value| now.saturating_sub(**value) <= 60_000)
            .count();
        if recent >= 3 || times.len() >= 20 {
            return Err(AppError {
                code: "plugin_notification_rate_limited",
                message: "插件通知过于频繁，请稍后重试".into(),
                details: Some(json!({"retryAfterMs":60_000})),
            });
        }
        times.push_back(now);
        let source = self
            .state
            .lock()
            .unwrap()
            .loaded
            .get(&format!("{plugin_id}/{component_id}"))
            .map(|loaded| loaded.name.clone())
            .unwrap_or_else(|| plugin_id.to_owned());
        Ok(json!({
            "kind": args.kind,
            "message": message,
            "durationMs": duration_ms,
            "source": source,
            "pluginId": plugin_id,
            "componentId": component_id,
            "action": action,
        }))
    }
}

fn method_matches(method: Method, capability: Capability) -> bool {
    matches!(
        (method, capability),
        (Method::LogWrite, Capability::LogsWrite)
            | (
                Method::StorageGet | Method::StorageList | Method::StorageExport,
                Capability::StorageRead
            )
            | (
                Method::StoragePut
                    | Method::StorageDelete
                    | Method::StorageClear
                    | Method::StorageMigrate,
                Capability::StorageWrite
            )
            | (Method::NotificationPost, Capability::NotificationsPost)
            | (
                Method::SchedulePut | Method::ScheduleList | Method::ScheduleDelete,
                Capability::TasksSchedule
            )
            | (Method::BrowserOpen, Capability::BrowserOpen)
            | (
                Method::CallbackCreate | Method::CallbackPoll | Method::CallbackCancel,
                Capability::BrowserCallback
            )
            | (Method::FileRead, Capability::FilesSelectionRead)
            | (Method::FileWrite, Capability::FilesSelectionWrite)
            | (
                Method::MaterialSubmit | Method::MaterialDrop,
                Capability::MaterialsSubmit
            )
            | (Method::SecretReceive, Capability::SecretsSessionReceive)
            | (
                Method::ConfiguredRequest,
                Capability::NetworkConfiguredRequest
            )
            | (Method::CryptoUse, Capability::CryptoSessionUse)
            | (
                Method::PersistentSecretGet,
                Capability::PersistentSecretsRead
            )
            | (
                Method::PersistentSecretPut | Method::PersistentSecretDelete,
                Capability::PersistentSecretsWrite
            )
            | (
                Method::CryptoKeyGenerate
                    | Method::CryptoSign
                    | Method::CryptoVerify
                    | Method::CryptoDigest
                    | Method::CryptoHpkeKeyGenerate
                    | Method::CryptoHpkeSeal
                    | Method::CryptoHpkeOpen,
                Capability::CryptoDeviceUse
            )
            | (
                Method::SubscriptionApply | Method::SubscriptionRemove,
                Capability::SubscriptionsManage
            )
            | (Method::ProtectedLoad, Capability::RuntimeProtectedLoad)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use znet_plugin_sandbox::distribution::package::{PageKind, VerifiedPage};

    #[test]
    fn plugin_logs_require_exact_capability_and_trusted_identity() {
        assert!(method_matches(Method::LogWrite, Capability::LogsWrite));
        assert!(!method_matches(Method::LogWrite, Capability::StorageWrite));
        let args: PluginLogArgs = serde_json::from_value(json!({
            "level": "warn", "message": "provider unavailable",
            "fields": {"pluginId": "forged", "token": "secret"}
        }))
        .unwrap();
        args.validate().unwrap();
        let fields = args.trusted_fields("real.plugin", "worker");
        assert_eq!(fields["pluginId"], "real.plugin");
        assert_eq!(fields["componentId"], "worker");
        assert_eq!(fields["data"]["pluginId"], "forged");
        assert!(serde_json::from_value::<PluginLogArgs>(json!({
            "level": "error", "message": "bad", "pluginId": "forged"
        }))
        .is_err());
        assert!(serde_json::from_value::<PluginLogArgs>(json!({
            "level": "fatal", "message": "bad"
        }))
        .is_err());
        assert!(serde_json::from_value::<PluginLogArgs>(json!({
            "level": "info", "message": "x".repeat(2_049)
        }))
        .unwrap()
        .validate()
        .is_err());
        assert!(serde_json::from_value::<PluginLogArgs>(json!({
            "level": "info", "message": "   "
        }))
        .unwrap()
        .validate()
        .is_err());
        assert!(serde_json::from_value::<PluginLogArgs>(json!({
            "level": "info", "message": "ready", "fields": {"large": "x".repeat(4_096)}
        }))
        .unwrap()
        .validate()
        .is_err());
    }

    #[test]
    fn notification_navigation_accepts_only_a_signed_page_and_safe_route() {
        let host = Host::default();
        let arguments = || {
            json!({
                "kind":"info",
                "message":"new message",
                "action":{"pageId":"manage","route":"messages","reference":"message-1"}
            })
        };
        assert!(host
            .notification("org.example.plugin", "worker", arguments())
            .is_err());
        host.state.lock().unwrap().pages.insert(
            "org.example.plugin".into(),
            BTreeMap::from([(
                "manage".into(),
                VerifiedPage {
                    id: "manage".into(),
                    title: "Manage".into(),
                    kind: PageKind::Management,
                    html: "<main></main>".into(),
                    styles: Vec::new(),
                    scripts: Vec::new(),
                },
            )]),
        );
        let value = host
            .notification("org.example.plugin", "worker", arguments())
            .unwrap();
        assert_eq!(value["action"]["reference"], "message-1");
        assert!(host
            .notification(
                "org.example.plugin",
                "worker",
                json!({"message":"bad","action":{"pageId":"manage","route":"../escape"}}),
            )
            .is_err());
    }
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> AppResult<T> {
    serde_json::from_value(value).map_err(|_| AppError::invalid_argument("插件 SDK 参数无效"))
}
