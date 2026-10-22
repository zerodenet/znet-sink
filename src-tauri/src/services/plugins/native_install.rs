//! Native selected bytes stay in the host. Confirmation consumes one bounded,
//! expiring plan; neither a guest path nor a retry can recreate its authority.
use super::*;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri_plugin_znet_native_io::{Error as NativeError, Receipt, ReceiptStatus};
use znet_plugin_sandbox::distribution::{package, MAX_PACKAGE_BYTES};

fn selection_unknown() -> AppError {
    AppError {
        code: "plugin_native_selection_unconfirmed",
        message: "系统文件读取结果未确认；不会安装或自动重试，请返回原操作重新核对".into(),
        details: None,
    }
}
/// Only validated native receipts enter here. No raw reason, path or provider
/// exception is copied to the page, and only `Read` can create an install plan.
pub(crate) fn selected_bytes(result: Result<Receipt, NativeError>) -> AppResult<Option<Vec<u8>>> {
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(NativeError::BridgeResultUnknown | NativeError::InvalidReceipt(_)) => {
            return Err(selection_unknown());
        }
        Err(NativeError::Busy) => {
            return Err(AppError::invalid_argument(
                "另一个系统文件操作尚未结束，请先完成原操作",
            ));
        }
        Err(NativeError::Unavailable) => {
            return Err(platform::unavailable("当前平台的系统插件包选择不可用"));
        }
        Err(NativeError::InvalidRequest(_)) => {
            return Err(AppError::invalid_argument(
                "系统插件包选择请求不符合宿主策略",
            ));
        }
    };
    match receipt.status {
        ReceiptStatus::Cancelled => Ok(None),
        ReceiptStatus::Read => {
            let bytes = receipt
                .read_bytes()
                .map_err(|_| selection_unknown())?
                .ok_or_else(selection_unknown)?;
            if bytes.is_empty() || bytes.len() > MAX_PACKAGE_BYTES {
                return Err(AppError::invalid_argument(
                    "插件包为空或超过 16 MiB，未开始安装",
                ));
            }
            Ok(Some(bytes))
        }
        ReceiptStatus::Failed => Err(AppError::invalid_argument(
            match receipt.reason.as_deref() {
                Some("byte-budget-exceeded" | "size-exceeded") => {
                    "插件包超过 16 MiB，未开始安装；请选择支持的包"
                }
                Some("document-permission-denied") => {
                    "系统未允许读取所选插件包，未开始安装；请重新选择或检查文件提供者权限"
                }
                Some("invalid-document-result") => {
                    "系统未返回有效的插件包文件，未开始安装；请重新选择"
                }
                _ => "所选插件包读取失败，未开始安装；请检查文件提供者后重新选择",
            },
        )),
        ReceiptStatus::ResultUnknown | ReceiptStatus::Saved | ReceiptStatus::ShareAccepted => {
            Err(selection_unknown())
        }
    }
}

const TTL: Duration = Duration::from_secs(300);
const CAPACITY: usize = 2;
struct Plan {
    created: Instant,
    bytes: Vec<u8>,
    review: InstallReview,
}
#[derive(Default)]
pub(super) struct Plans(Mutex<BTreeMap<String, Plan>>);
#[derive(Serialize)]
pub struct Preview {
    pub plan_id: String,
    pub review: InstallReview,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectionTarget {
    pub plugin_id: String,
    pub candidate_digest: String,
    pub candidate_version: String,
    pub publisher_fingerprint: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub plugin_id: String,
    pub state: &'static str,
    pub installed_version: Option<String>,
    pub package_digest: Option<String>,
    pub observed_at_unix_ms: String,
}
fn expired() -> AppError {
    AppError::invalid_argument("系统选择的安装审阅已失效，请重新选择并核对插件包")
}
impl Plans {
    pub(super) fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
    fn insert(&self, bytes: Vec<u8>, review: InstallReview) -> AppResult<Preview> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(expired());
        }
        let mut plans = self.0.lock().map_err(|_| expired())?;
        plans.retain(|_, plan| plan.created.elapsed() < TTL);
        if plans.len() >= CAPACITY {
            return Err(AppError::invalid_argument(
                "已有待确认插件包，请先取消或完成原审阅",
            ));
        }
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(|_| AppError::internal("无法创建安装审阅凭据"))?;
        let plan_id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        plans.insert(
            plan_id.clone(),
            Plan {
                created: Instant::now(),
                bytes,
                review: review.clone(),
            },
        );
        Ok(Preview { plan_id, review })
    }
    fn take(&self, id: &str) -> AppResult<Plan> {
        let plan = self
            .0
            .lock()
            .map_err(|_| expired())?
            .remove(id)
            .ok_or_else(expired)?;
        if plan.created.elapsed() >= TTL {
            return Err(expired());
        }
        Ok(plan)
    }
}
impl Host {
    /// Re-read verified local material only. Does not download, install, grant,
    /// enable, or infer execution health; an old review is a read target only.
    pub fn inspect_native_installation(
        &self,
        manager: &Manager,
        target: InspectionTarget,
    ) -> AppResult<Inspection> {
        let valid_digest = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if target.plugin_id.is_empty()
            || target.plugin_id.len() > 256
            || target.candidate_version.len() > 128
            || semver::Version::parse(&target.candidate_version).is_err()
            || !valid_digest(&target.candidate_digest)
            || !valid_digest(&target.publisher_fingerprint)
        {
            return Err(expired());
        }
        let _operation = self.operation_guard()?;
        io::manage(manager, || {
            let store = self.store().map_err(|_| inspection_unavailable())?;
            let mut result = Inspection {
                plugin_id: target.plugin_id.clone(),
                state: "not-installed",
                installed_version: None,
                package_digest: None,
                observed_at_unix_ms: crate::services::common::now_unix_ms().to_string(),
            };
            if !store
                .list()
                .map_err(|_| inspection_unavailable())?
                .contains(&target.plugin_id)
            {
                return Ok(result);
            }
            let root = self.root().map_err(|_| inspection_unavailable())?;
            let directory = local_state::effective_directory(
                &root,
                local_state::load_directory(&root).map_err(|_| inspection_unavailable())?,
            )
            .map_err(|_| inspection_unavailable())?
            .ok_or_else(inspection_unavailable)?;
            let registration = directory
                .find(&target.plugin_id)
                .map_err(|_| inspection_unavailable())?;
            let (review, package) = store
                .review_uninstall(registration)
                .map_err(|_| inspection_unavailable())?;
            result.state = if review.package_digest == target.candidate_digest
                && package.version == target.candidate_version
                && publisher_fingerprint(registration).map_err(|_| inspection_unavailable())?
                    == target.publisher_fingerprint
            {
                "matching"
            } else {
                "different"
            };
            result.installed_version = Some(package.version);
            result.package_digest = Some(review.package_digest);
            Ok(result)
        })
    }
    pub fn require_native_installation(&self) -> AppResult<()> {
        if !cfg!(any(target_os = "android", target_os = "ios")) {
            return Err(platform::unavailable("此入口仅用于移动系统文件选择"));
        }
        self.platform.require_installation()
    }
    pub fn preview_native_package(&self, manager: &Manager, bytes: Vec<u8>) -> AppResult<Preview> {
        let _operation = self.operation_guard()?;
        self.platform.require_installation()?;
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(expired());
        }
        let id = package::package_id(&bytes).map_err(io::failure)?;
        let review = self.preview_local_bytes(manager, &bytes, &id)?;
        self.platform
            .check_review_generation(review.host_generation.as_deref())?;
        self.native_installs.insert(bytes, review)
    }
    pub fn cancel_native_package(&self, id: &str) {
        let _ = self.native_installs.take(id);
    }
    pub fn install_native_package(
        &self,
        manager: &Manager,
        plan_id: &str,
        approved_digest: &str,
    ) -> AppResult<Snapshot> {
        // Consume even on failure. A lost receipt is reconciled from installed
        // state, never replayed with new authorization or a newly selected file.
        let plan = self.native_installs.take(plan_id)?;
        let _operation = self.operation_guard()?;
        self.platform
            .check_review_generation(plan.review.host_generation.as_deref())?;
        self.platform.require_installation()?;
        if approved_digest != plan.review.candidate_digest {
            return Err(expired());
        }
        let current = self.preview_local_bytes(manager, &plan.bytes, &plan.review.plugin_id)?;
        if current.current_version != plan.review.current_version
            || current.publisher_fingerprint != plan.review.publisher_fingerprint
            || current.candidate_digest != plan.review.candidate_digest
        {
            return Err(expired());
        }
        self.install_local_bytes(
            manager,
            &plan.bytes,
            &plan.review.plugin_id,
            Some(approved_digest),
        )
    }
}
fn inspection_unavailable() -> AppError {
    AppError {
        code: "plugin_install_verification_unavailable",
        message: "当前安装材料、登记或签名未通过核对；结果仍未确认，不会重放安装".into(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use znet_plugin_sandbox::distribution::package::{Payload, SourceComponent};

    fn receipt(status: ReceiptStatus, reason: Option<&str>) -> Receipt {
        Receipt {
            operation_id: "owned-native-read".into(),
            status,
            bytes_read: None,
            content_base64: None,
            reason: reason.map(str::to_owned),
        }
    }
    #[test]
    fn native_read_cancel_failure_and_unknown_never_become_package_bytes() {
        assert!(selected_bytes(Ok(receipt(ReceiptStatus::Cancelled, None)))
            .unwrap()
            .is_none());
        for status in [
            ReceiptStatus::ResultUnknown,
            ReceiptStatus::Saved,
            ReceiptStatus::ShareAccepted,
        ] {
            let error = selected_bytes(Ok(receipt(status, Some("private-token")))).unwrap_err();
            assert_eq!(error.code, "plugin_native_selection_unconfirmed");
            assert!(error.details.is_none());
            assert!(!error.message.contains("private-token"));
        }
        for (reason, message) in [
            ("byte-budget-exceeded", "16 MiB"),
            ("size-exceeded", "16 MiB"),
            ("document-permission-denied", "系统未允许"),
            ("invalid-document-result", "有效"),
            ("content://private/token", "读取失败"),
        ] {
            let error =
                selected_bytes(Ok(receipt(ReceiptStatus::Failed, Some(reason)))).unwrap_err();
            assert_eq!(error.code, "invalid_argument");
            assert!(error.message.contains(message));
            assert!(error.message.contains("未开始安装"));
            assert!(error.details.is_none());
            assert!(!error.message.contains(reason));
        }
    }
    #[test]
    fn native_read_bridge_errors_preserve_unknown_and_busy_without_raw_details() {
        for source in [
            NativeError::BridgeResultUnknown,
            NativeError::InvalidReceipt("private-token"),
        ] {
            let error = selected_bytes(Err(source)).unwrap_err();
            assert_eq!(error.code, "plugin_native_selection_unconfirmed");
            assert!(!error.message.contains("private-token"));
        }
        assert!(selected_bytes(Err(NativeError::Busy))
            .unwrap_err()
            .message
            .contains("原操作"));
        assert_eq!(
            selected_bytes(Err(NativeError::Unavailable))
                .unwrap_err()
                .code,
            "plugin_platform_unavailable"
        );
        assert!(
            !selected_bytes(Err(NativeError::InvalidRequest("private-token")))
                .unwrap_err()
                .message
                .contains("private-token")
        );
    }
    #[test]
    fn only_complete_nonempty_bounded_native_read_can_reach_package_preflight() {
        let mut read = receipt(ReceiptStatus::Read, None);
        read.bytes_read = Some(3);
        read.content_base64 = Some(STANDARD.encode(b"zip"));
        assert_eq!(selected_bytes(Ok(read.clone())).unwrap().unwrap(), b"zip");
        read.bytes_read = Some(4);
        assert_eq!(
            selected_bytes(Ok(read.clone())).unwrap_err().code,
            "plugin_native_selection_unconfirmed"
        );
        read.bytes_read = Some(0);
        read.content_base64 = Some(String::new());
        assert!(selected_bytes(Ok(read))
            .unwrap_err()
            .message
            .contains("为空"));
    }
    fn selected() -> Vec<u8> {
        let seed = [42; 32];
        let registration: Registration = serde_json::from_value(serde_json::json!({"id":"org.example.native","repository":"https://github.com/example/native","publisher":{"id":"example","public_key":STANDARD.encode(ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes())},"name":"Native test","description":"Test","license":"MIT","maintainers":["example"],"release_source":{"type":"github-releases","metadata_asset":"marketplace-entry.json"},"surfaces":[],"capabilities":[]})).unwrap();
        let source = "42";
        let manifest = serde_json::from_value(serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.native","component_id":"main","version":"1.0.0","requires_host":format!("={}",env!("CARGO_PKG_VERSION")),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[],"optional":[],"source_sha256":znet_plugin_sandbox::contract::sha256(source.as_bytes()),"limits":znet_plugin_sandbox::contract::Limits::default()})).unwrap();
        let payload = Payload {
            schema_version: 1,
            host: "znet-sink".into(),
            plugin_id: "org.example.native".into(),
            version: "1.0.0".into(),
            components: vec![SourceComponent {
                manifest,
                source: source.into(),
            }],
            pages: vec![],
        };
        package::sign_with_registration(&serde_json::to_vec(&payload).unwrap(), &seed, registration)
            .unwrap()
    }
    fn setup() -> (tempfile::TempDir, Host, Manager) {
        let root = tempfile::tempdir().unwrap();
        let host = Host {
            root: Some(root.path().into()),
            ..Default::default()
        };
        (root, host, Manager::default())
    }
    fn inspection_target(review: &InstallReview) -> InspectionTarget {
        InspectionTarget {
            plugin_id: review.plugin_id.clone(),
            candidate_digest: review.candidate_digest.clone(),
            candidate_version: review.candidate_version.clone(),
            publisher_fingerprint: review.publisher_fingerprint.clone(),
        }
    }
    #[test]
    fn installation_inspection_uses_local_signature_and_original_digest_without_replay() {
        let (root, host, manager) = setup();
        let preview = host.preview_native_package(&manager, selected()).unwrap();
        assert_eq!(
            host.inspect_native_installation(&manager, inspection_target(&preview.review))
                .unwrap()
                .state,
            "not-installed"
        );
        host.install_native_package(&manager, &preview.plan_id, &preview.review.candidate_digest)
            .unwrap();
        // No in-memory catalog is needed: this read verifies the saved local registration.
        host.state.lock().unwrap().directory = None;
        let found = host
            .inspect_native_installation(&manager, inspection_target(&preview.review))
            .unwrap();
        assert_eq!(found.state, "matching");
        assert_eq!(found.installed_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            found.package_digest.as_deref(),
            Some(preview.review.candidate_digest.as_str())
        );
        assert!(!found.observed_at_unix_ms.is_empty());
        assert!(host.state.lock().unwrap().directory.is_none()); // Read did not enable or rescan.
        let mut different = inspection_target(&preview.review);
        different.candidate_digest = "0".repeat(64);
        assert_eq!(
            host.inspect_native_installation(&manager, different)
                .unwrap()
                .state,
            "different"
        );
        assert!(host.native_installs.take(&preview.plan_id).is_err());
        let blob = root
            .path()
            .join("packages")
            .join(format!("{}.zspkg", preview.review.candidate_digest));
        std::fs::write(&blob, b"private-corrupt-token").unwrap();
        let error = host
            .inspect_native_installation(&manager, inspection_target(&preview.review))
            .unwrap_err();
        assert_eq!(error.code, "plugin_install_verification_unavailable");
        assert!(!error.message.contains("private-corrupt-token"));
        assert!(error.details.is_none());
        assert_eq!(std::fs::read(blob).unwrap(), b"private-corrupt-token");
    }
    #[test]
    fn selected_signed_bytes_install_once_after_original_publisher_review() {
        let (_root, host, manager) = setup();
        let preview = host.preview_native_package(&manager, selected()).unwrap();
        assert!(
            preview.review.local_trust
                && preview.review.first_install
                && preview.review.requires_approval
        );
        let snapshot = host
            .install_native_package(&manager, &preview.plan_id, &preview.review.candidate_digest)
            .unwrap();
        assert_eq!(snapshot.components.len(), 1);
        assert!(!snapshot.components[0].enabled);
        assert!(host
            .install_native_package(&manager, &preview.plan_id, &preview.review.candidate_digest)
            .is_err());
    }
    #[test]
    fn wrong_digest_consumes_authority_and_never_installs() {
        let (_root, host, manager) = setup();
        let preview = host.preview_native_package(&manager, selected()).unwrap();
        assert!(host
            .install_native_package(&manager, &preview.plan_id, "wrong")
            .is_err());
        assert!(host
            .install_native_package(&manager, &preview.plan_id, &preview.review.candidate_digest)
            .is_err());
        assert!(host.store().unwrap().list().unwrap().is_empty());
    }
    #[test]
    fn selection_budget_expiry_cancellation_and_replacement_prevent_replay() {
        let (_root, host, manager) = setup();
        let first = host.preview_native_package(&manager, selected()).unwrap();
        let second = host.preview_native_package(&manager, selected()).unwrap();
        assert!(host.preview_native_package(&manager, selected()).is_err());
        host.cancel_native_package(&first.plan_id);
        assert!(host
            .install_native_package(&manager, &first.plan_id, &first.review.candidate_digest)
            .is_err());
        let third = host.preview_native_package(&manager, selected()).unwrap();
        host.native_installs
            .0
            .lock()
            .unwrap()
            .get_mut(&third.plan_id)
            .unwrap()
            .created = Instant::now() - TTL;
        assert!(host
            .install_native_package(&manager, &third.plan_id, &third.review.candidate_digest)
            .is_err());
        let fourth = host.preview_native_package(&manager, selected()).unwrap();
        host.install_native_package(&manager, &second.plan_id, &second.review.candidate_digest)
            .unwrap();
        assert!(host
            .install_native_package(&manager, &fourth.plan_id, &fourth.review.candidate_digest)
            .is_err());
        host.native_installs.clear();
        assert!(host.native_installs.0.lock().unwrap().is_empty());
    }
}
