use super::*;
use std::cell::RefCell;

#[test]
fn dns_failure_still_cleans_proxy_and_retains_both_errors() {
    let calls = RefCell::new(Vec::new());
    let error = release_capture(
        true,
        || {
            calls.borrow_mut().push("dns");
            Err(AppError::internal("DNS permission denied"))
        },
        || {
            calls.borrow_mut().push("proxy");
            Err(AppError::internal("proxy permission denied"))
        },
    )
    .unwrap_err();
    assert_eq!(calls.into_inner(), ["dns", "proxy"]);
    assert!(error.message.contains("DNS permission denied"));
    assert!(error.message.contains("proxy permission denied"));
}

#[test]
fn proxy_opt_out_preserves_proxy_but_still_releases_dns() {
    let called = std::cell::Cell::new(false);
    release_capture(
        false,
        || {
            called.set(true);
            Ok(())
        },
        || panic!("proxy cleanup is opted out"),
    )
    .unwrap();
    assert!(called.get());
}
