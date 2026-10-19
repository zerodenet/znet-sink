//! Native selected bytes stay in the host. Confirmation consumes one bounded,
//! expiring plan; neither a guest path nor a retry can recreate its authority.
use super::*;
use serde::Serialize;
use std::time::{Duration, Instant};
use znet_plugin_sandbox::distribution::{package, MAX_PACKAGE_BYTES};

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

#[cfg(test)]
mod tests {
    use super::*;
    use znet_plugin_sandbox::distribution::package::{Payload, SourceComponent};
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
