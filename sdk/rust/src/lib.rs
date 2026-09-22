//! Public ZNet Sink plugin SDK v1.
//!
//! The host derives plugin and component identity from the signed execution
//! context. The SDK accepts no host command name, plugin id, component id, or
//! raw filesystem path.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const SDK_VERSION: u32 = 1;
pub const MAX_ARGUMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "plugin.self.read")]
    SelfRead,
    #[serde(rename = "records.summary.read")]
    RecordsSummaryRead,
    #[serde(rename = "network.get")]
    NetworkGet,
    #[serde(rename = "network.request")]
    NetworkRequest,
    #[serde(rename = "network.configured.request")]
    NetworkConfiguredRequest,
    #[serde(rename = "plugin.storage.read")]
    StorageRead,
    #[serde(rename = "plugin.storage.write")]
    StorageWrite,
    #[serde(rename = "plugin.logs.write")]
    LogsWrite,
    #[serde(rename = "notifications.post")]
    NotificationsPost,
    #[serde(rename = "tasks.schedule")]
    TasksSchedule,
    #[serde(rename = "browser.open")]
    BrowserOpen,
    #[serde(rename = "browser.callback")]
    BrowserCallback,
    #[serde(rename = "files.selection.read")]
    FilesSelectionRead,
    #[serde(rename = "files.selection.write")]
    FilesSelectionWrite,
    #[serde(rename = "materials.submit")]
    MaterialsSubmit,
    #[serde(rename = "secrets.session.receive")]
    SecretsSessionReceive,
    #[serde(rename = "crypto.session.use")]
    CryptoSessionUse,
    #[serde(rename = "secrets.persistent.read")]
    PersistentSecretsRead,
    #[serde(rename = "secrets.persistent.write")]
    PersistentSecretsWrite,
    #[serde(rename = "crypto.device.use")]
    CryptoDeviceUse,
    #[serde(rename = "subscriptions.manage")]
    SubscriptionsManage,
    #[serde(rename = "runtime.protected.load")]
    RuntimeProtectedLoad,
}

impl Capability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelfRead => "plugin.self.read",
            Self::RecordsSummaryRead => "records.summary.read",
            Self::NetworkGet => "network.get",
            Self::NetworkRequest => "network.request",
            Self::NetworkConfiguredRequest => "network.configured.request",
            Self::StorageRead => "plugin.storage.read",
            Self::StorageWrite => "plugin.storage.write",
            Self::LogsWrite => "plugin.logs.write",
            Self::NotificationsPost => "notifications.post",
            Self::TasksSchedule => "tasks.schedule",
            Self::BrowserOpen => "browser.open",
            Self::BrowserCallback => "browser.callback",
            Self::FilesSelectionRead => "files.selection.read",
            Self::FilesSelectionWrite => "files.selection.write",
            Self::MaterialsSubmit => "materials.submit",
            Self::SecretsSessionReceive => "secrets.session.receive",
            Self::CryptoSessionUse => "crypto.session.use",
            Self::PersistentSecretsRead => "secrets.persistent.read",
            Self::PersistentSecretsWrite => "secrets.persistent.write",
            Self::CryptoDeviceUse => "crypto.device.use",
            Self::SubscriptionsManage => "subscriptions.manage",
            Self::RuntimeProtectedLoad => "runtime.protected.load",
        }
    }

    pub fn accepts_scope(self, scope: &str) -> bool {
        match self {
            Self::SelfRead
            | Self::StorageRead
            | Self::StorageWrite
            | Self::LogsWrite
            | Self::NotificationsPost
            | Self::TasksSchedule
            | Self::BrowserCallback
            | Self::CryptoSessionUse
            | Self::PersistentSecretsRead
            | Self::PersistentSecretsWrite
            | Self::CryptoDeviceUse
            | Self::SubscriptionsManage => scope == "self",
            Self::FilesSelectionRead | Self::FilesSelectionWrite => scope == "user",
            Self::MaterialsSubmit => scope == "configuration",
            Self::RuntimeProtectedLoad => scope == "active-runtime",
            Self::NetworkGet
            | Self::NetworkRequest
            | Self::BrowserOpen
            | Self::SecretsSessionReceive => exact_http_origin(scope),
            Self::NetworkConfiguredRequest => identifier(scope),
            Self::RecordsSummaryRead => scope.strip_prefix("selection:").is_some_and(identifier),
        }
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn exact_http_origin(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return false;
    }
    let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap());
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    value == origin
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capability: Capability,
    pub scope: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_result_bytes")]
    pub max_result_bytes: usize,
}
const fn default_timeout_ms() -> u64 {
    30_000
}
const fn default_result_bytes() -> usize {
    MAX_ARGUMENT_BYTES
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            timeout_ms: default_timeout_ms(),
            max_result_bytes: default_result_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    LogWrite,
    StorageGet,
    StoragePut,
    StorageDelete,
    StorageList,
    StorageExport,
    StorageClear,
    StorageMigrate,
    NotificationPost,
    SchedulePut,
    ScheduleList,
    ScheduleDelete,
    BrowserOpen,
    CallbackCreate,
    CallbackPoll,
    CallbackCancel,
    FileRead,
    FileWrite,
    MaterialSubmit,
    MaterialDrop,
    SecretReceive,
    ConfiguredRequest,
    CryptoUse,
    PersistentSecretGet,
    PersistentSecretPut,
    PersistentSecretDelete,
    CryptoKeyGenerate,
    CryptoSign,
    CryptoVerify,
    CryptoDigest,
    CryptoHpkeKeyGenerate,
    CryptoHpkeSeal,
    CryptoHpkeOpen,
    SubscriptionApply,
    SubscriptionRemove,
    ProtectedLoad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub version: u32,
    pub request: Request,
    pub method: Method,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default)]
    pub arguments: Value,
}
impl Call {
    pub fn validate(&self) -> bool {
        self.version == SDK_VERSION
            && self.request.capability.accepts_scope(&self.request.scope)
            && (1..=MAX_TIMEOUT_MS).contains(&self.budget.timeout_ms)
            && (1..=MAX_RESULT_BYTES).contains(&self.budget.max_result_bytes)
            && serde_json::to_vec(&self.arguments)
                .is_ok_and(|value| value.len() <= MAX_ARGUMENT_BYTES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unsupported,
    PermissionDenied,
    Disabled,
    Revoked,
    Busy,
    Expired,
    Cancelled,
    Deadline,
    BudgetExceeded,
    NotFound,
    Transport,
    Uncertain,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u32,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}
impl Reply {
    pub fn success(value: Value) -> Self {
        Self {
            version: SDK_VERSION,
            ok: true,
            value: Some(value),
            error: None,
        }
    }
}

pub trait Transport {
    type Error;
    fn send(&self, call: Call) -> Result<Reply, Self::Error>;
}
pub struct Client<T> {
    transport: T,
    budget: Budget,
}
impl<T: Transport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            budget: Budget::default(),
        }
    }
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }
    pub fn transport(&self) -> &T {
        &self.transport
    }
    pub fn call(
        &self,
        capability: Capability,
        scope: impl Into<String>,
        method: Method,
        arguments: Value,
    ) -> Result<Reply, T::Error> {
        self.transport.send(Call {
            version: SDK_VERSION,
            request: Request {
                capability,
                scope: scope.into(),
            },
            method,
            budget: self.budget,
            arguments,
        })
    }
    pub fn storage_get(&self, area: &str, key: &str) -> Result<Reply, T::Error> {
        self.call(
            Capability::StorageRead,
            "self",
            Method::StorageGet,
            json!({"area":area,"key":key}),
        )
    }
    pub fn log(&self, level: &str, message: &str, fields: Value) -> Result<Reply, T::Error> {
        self.call(
            Capability::LogsWrite,
            "self",
            Method::LogWrite,
            json!({"level": level, "message": message, "fields": fields}),
        )
    }
    pub fn storage_put(&self, area: &str, key: &str, value: &str) -> Result<Reply, T::Error> {
        self.call(
            Capability::StorageWrite,
            "self",
            Method::StoragePut,
            json!({"area":area,"key":key,"value":value}),
        )
    }
    pub fn notify(&self, message: &str, kind: &str) -> Result<Reply, T::Error> {
        self.call(
            Capability::NotificationsPost,
            "self",
            Method::NotificationPost,
            json!({"message":message,"kind":kind}),
        )
    }
    pub fn schedule(
        &self,
        task_id: &str,
        action: &str,
        interval_seconds: u64,
    ) -> Result<Reply, T::Error> {
        self.call(
            Capability::TasksSchedule,
            "self",
            Method::SchedulePut,
            json!({"taskId":task_id,"action":action,"intervalSeconds":interval_seconds}),
        )
    }
    pub fn open_browser(&self, origin: &str, url: &str) -> Result<Reply, T::Error> {
        self.call(
            Capability::BrowserOpen,
            origin,
            Method::BrowserOpen,
            json!({"url":url}),
        )
    }
    pub fn create_callback(&self) -> Result<Reply, T::Error> {
        self.call(
            Capability::BrowserCallback,
            "self",
            Method::CallbackCreate,
            json!({}),
        )
    }
    pub fn read_user_file(&self, max_bytes: usize) -> Result<Reply, T::Error> {
        self.call(
            Capability::FilesSelectionRead,
            "user",
            Method::FileRead,
            json!({"maxBytes":max_bytes}),
        )
    }
    pub fn submit_material(
        &self,
        data_base64: &str,
        purpose: &str,
        lifetime_ms: u64,
    ) -> Result<Reply, T::Error> {
        self.call(
            Capability::MaterialsSubmit,
            "configuration",
            Method::MaterialSubmit,
            json!({"dataBase64":data_base64,"purpose":purpose,"lifetimeMs":lifetime_ms}),
        )
    }
    pub fn receive_secret(&self, origin: &str, arguments: Value) -> Result<Reply, T::Error> {
        self.call(
            Capability::SecretsSessionReceive,
            origin,
            Method::SecretReceive,
            arguments,
        )
    }
    pub fn crypto(&self, arguments: Value) -> Result<Reply, T::Error> {
        self.call(
            Capability::CryptoSessionUse,
            "self",
            Method::CryptoUse,
            arguments,
        )
    }
    pub fn load_protected_runtime(&self, handle: &str) -> Result<Reply, T::Error> {
        self.call(
            Capability::RuntimeProtectedLoad,
            "active-runtime",
            Method::ProtectedLoad,
            json!({"handle":handle}),
        )
    }
}
