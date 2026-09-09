use std::{collections::VecDeque, sync::Mutex};
use znet_engine_client::configuration::{confirm, ApplyFailure, ConfigBackend, RuntimeIdentity};
struct Backend {
    identities: Mutex<VecDeque<Result<RuntimeIdentity, &'static str>>>,
    result: Result<RuntimeIdentity, &'static str>,
    calls: Mutex<Vec<&'static str>>,
}
impl ConfigBackend for Backend {
    type Error = &'static str;
    async fn identity(&self) -> Result<RuntimeIdentity, Self::Error> {
        self.calls.lock().unwrap().push("identity");
        self.identities.lock().unwrap().pop_front().unwrap()
    }
    async fn submit(&self) -> Result<RuntimeIdentity, Self::Error> {
        self.calls.lock().unwrap().push("submit");
        self.result.clone()
    }
}
fn id(core: &str, revision: u64) -> RuntimeIdentity {
    RuntimeIdentity {
        core_instance_id: core.into(),
        config_revision: revision,
    }
}
fn backend(
    result: Result<RuntimeIdentity, &'static str>,
    current: Result<RuntimeIdentity, &'static str>,
) -> Backend {
    Backend {
        identities: Mutex::new([Ok(id("a", 1)), current].into()),
        result,
        calls: Mutex::new(vec![]),
    }
}
#[tokio::test]
async fn only_matching_instance_and_revision_produce_a_commit_receipt() {
    for (applied, current, success) in [
        (id("a", 2), id("a", 2), true),
        (id("b", 1), id("b", 1), false),
        (id("a", 2), id("a", 3), false),
        (id("a", 2), id("b", 2), false),
    ] {
        let b = backend(Ok(applied), Ok(current.clone()));
        match confirm(&b).await {
            Ok(receipt) => {
                assert!(success);
                assert_eq!(receipt.identity(), &current)
            }
            Err(ApplyFailure::IdentityChanged) => assert!(!success),
            _ => panic!("unexpected result"),
        }
        assert_eq!(
            *b.calls.lock().unwrap(),
            vec!["identity", "submit", "identity"]
        );
    }
}
#[tokio::test]
async fn lost_reply_and_rejection_are_never_replayed() {
    for failure in ["lost_reply", "rejected"] {
        let b = backend(Err(failure), Ok(id("a", 2)));
        assert!(matches!(
            confirm(&b).await,
            Err(ApplyFailure::Submission(_))
        ));
        assert_eq!(*b.calls.lock().unwrap(), vec!["identity", "submit"]);
    }
}
#[tokio::test]
async fn failed_preflight_never_submits_and_failed_readback_cannot_commit() {
    let b = backend(Ok(id("a", 2)), Err("offline"));
    assert!(matches!(
        confirm(&b).await,
        Err(ApplyFailure::Confirmation("offline"))
    ));
    let b = backend(Ok(id("a", 2)), Ok(id("a", 2)));
    b.identities.lock().unwrap()[0] = Err("offline");
    assert!(matches!(
        confirm(&b).await,
        Err(ApplyFailure::Preflight("offline"))
    ));
    assert_eq!(*b.calls.lock().unwrap(), vec!["identity"]);
}
