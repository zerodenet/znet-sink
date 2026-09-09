use std::sync::{Arc, Mutex};

use gui_lib::lifecycle::{shutdown::ShutdownCoordinator, Phase};

#[test]
fn shutdown_releases_each_phase_in_reverse_registration_order() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut shutdown = ShutdownCoordinator::new();
    for (phase, name) in [
        (Phase::Guard, "guard"),
        (Phase::Runtime, "runtime-first"),
        (Phase::State, "state"),
        (Phase::Runtime, "runtime-last"),
    ] {
        let calls = Arc::clone(&calls);
        shutdown.register(
            phase,
            name,
            Box::new(move || calls.lock().unwrap().push(name)),
        );
    }

    shutdown.run();

    assert_eq!(
        *calls.lock().unwrap(),
        ["runtime-last", "runtime-first", "state", "guard"]
    );
}

#[test]
fn panicking_cleanup_does_not_skip_remaining_cleanup() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut shutdown = ShutdownCoordinator::new();
    let guard_calls = Arc::clone(&calls);
    shutdown.register(
        Phase::Guard,
        "system-cleanup",
        Box::new(move || guard_calls.lock().unwrap().push("system-cleanup")),
    );
    shutdown.register(
        Phase::Runtime,
        "failed-service",
        Box::new(|| panic!("fixture")),
    );

    shutdown.run();

    assert_eq!(*calls.lock().unwrap(), ["system-cleanup"]);
}

#[test]
fn shutdown_coordinator_is_send_and_sync_without_manual_impls() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ShutdownCoordinator>();
}
