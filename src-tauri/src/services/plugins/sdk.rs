//! Business-neutral plugin SDK execution for signed management pages.

use super::*;
use crate::services::common::now_unix_ms;
use base64::{engine::general_purpose::STANDARD, Engine};
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

impl Host {
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
        if !call.validate() || !method_matches(call.method, call.request.capability) {
            return Err(AppError::invalid_argument("插件 SDK 请求格式或能力不匹配"));
        }
        let budget = call.budget;
        let started = Instant::now();
        let (component, authority) =
            self.sdk_context(manager, plugin_id, component_id, &call.request)?;
        let lease = authority.begin(&component).map_err(io::failure)?;
        lease.check(Some(&call.request)).map_err(io::failure)?;
        let value = match call.method {
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
        let lease = authority.begin(&component).map_err(io::failure)?;
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
        Ok(json!({
            "kind": args.kind,
            "message": message,
            "durationMs": duration_ms,
            "source": plugin_id,
        }))
    }
}

fn method_matches(method: Method, capability: Capability) -> bool {
    matches!(
        (method, capability),
        (
            Method::StorageGet | Method::StorageList | Method::StorageExport,
            Capability::StorageRead
        ) | (
            Method::StoragePut
                | Method::StorageDelete
                | Method::StorageClear
                | Method::StorageMigrate,
            Capability::StorageWrite
        ) | (Method::NotificationPost, Capability::NotificationsPost)
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
            | (Method::CryptoUse, Capability::CryptoSessionUse)
            | (Method::ProtectedLoad, Capability::RuntimeProtectedLoad)
    )
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> AppResult<T> {
    serde_json::from_value(value).map_err(|_| AppError::invalid_argument("插件 SDK 参数无效"))
}
