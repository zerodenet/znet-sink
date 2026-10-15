use std::time::Duration;
use znet_engine_client::{Binding, Endpoint, SubscriptionOwner};
fn binding(path: &str) -> Binding {
    Binding {
        endpoint: Endpoint {
            transport: "test",
            path: path.into(),
        },
        timeout: Duration::from_secs(1),
    }
}
#[test]
fn switching_endpoint_invalidates_old_work_and_preserves_its_original_binding() {
    let owner = SubscriptionOwner::default();
    let first = owner.begin(binding("first"));
    let second = owner.begin(binding("second"));
    assert!(!first.is_current());
    assert_eq!(first.binding.endpoint.path, "first");
    assert!(second.is_current());
    assert_eq!(owner.binding().unwrap().endpoint.path, "second");
}
#[test]
fn stopping_returns_only_the_owned_endpoint_and_cancels_all_clones() {
    let owner = SubscriptionOwner::default();
    let sub = owner.begin(binding("selected"));
    let clone = sub.clone();
    let (generation, stopped) = owner.stop();
    assert!(generation > sub.generation);
    assert!(!sub.is_current());
    assert!(!clone.is_current());
    assert_eq!(stopped.unwrap().endpoint.path, "selected");
    assert!(owner.binding().is_none());
    assert!(owner.stop().1.is_none());
    assert!(owner.begin(binding("next")).is_current());
}
#[test]
fn independent_owners_do_not_cancel_each_other() {
    let a = SubscriptionOwner::default();
    let b = SubscriptionOwner::default();
    let sub = b.begin(binding("b"));
    a.begin(binding("a"));
    a.stop();
    assert!(sub.is_current());
}

#[test]
fn current_is_a_read_only_coherent_lease_and_same_endpoint_aba_cannot_revive_it() {
    let owner = SubscriptionOwner::default();
    assert!(owner.current().is_none());
    let original = owner.begin(binding("same"));
    let first = owner.current().unwrap();
    let second = owner.current().unwrap();
    assert_eq!(first.generation, original.generation);
    assert_eq!(second.generation, original.generation);
    assert_eq!(first.binding.endpoint, original.binding.endpoint);
    assert!(first.is_current());
    owner.stop();
    assert!(owner.current().is_none());
    assert!(!first.is_current());
    let replacement = owner.begin(binding("same"));
    assert!(replacement.generation > first.generation);
    assert!(owner.current().unwrap().is_current());
    assert!(!second.is_current());
}
