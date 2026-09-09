use super::owns_endpoint;
#[test]
fn restoration_requires_current_ownership_not_just_a_historical_marker() {
    assert!(owns_endpoint(true, "127.0.0.1", 1080, "127.0.0.1", 1080));
    assert!(!owns_endpoint(false, "127.0.0.1", 1080, "127.0.0.1", 1080));
    assert!(!owns_endpoint(true, "127.0.0.1", 7890, "127.0.0.1", 1080));
    assert!(!owns_endpoint(true, "192.0.2.1", 1080, "127.0.0.1", 1080));
}
