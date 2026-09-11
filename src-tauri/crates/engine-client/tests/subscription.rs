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
