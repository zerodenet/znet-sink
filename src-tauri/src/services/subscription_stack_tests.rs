use super::*;
use std::future::Future;

fn future_size<A, F: Future>(_: impl FnOnce(A) -> F) -> usize {
    std::mem::size_of::<F>()
}

#[test]
fn subscription_stack_future_sizes() {
    let sizes = [
        (
            "proxy mode command",
            future_size(|(a, b, c)| crate::commands::proxy_mode::gui_set_proxy_mode(a, b, c)),
            2048,
        ),
        (
            "subscription command",
            future_size(|(app, id)| crate::commands::subscription::subscription_sync(app, id)),
            2048,
        ),
        (
            "sync all command",
            future_size(crate::commands::subscription::subscription_sync_all),
            2048,
        ),
        (
            "subscription wrapper",
            future_size(|(app, id)| crate::services::subscription::sync(app, id)),
            8192,
        ),
        (
            "subscription original",
            future_size(|(app, id)| sync(app, id)),
            8192,
        ),
        (
            "capture apply",
            future_size(|(state, source, profile, config, options)| {
                crate::configuration::capture_apply::apply_profile(
                    state, source, profile, config, options,
                )
            }),
            8192,
        ),
    ];
    for (label, size, budget) in sizes {
        eprintln!("{label}: {size} bytes (budget {budget})");
        assert!(size <= budget, "{label} embeds {size} bytes; heap-allocate phase state before crossing IPC/publisher boundaries");
    }
}

#[path = "subscription/refresh_stack_tests.rs"]
mod refresh;
