use super::*;
use std::sync::{atomic::AtomicUsize, Arc, Barrier};

#[test]
fn stopped_during_backoff_cancels_monitor_even_without_a_child() {
    let host = Host::default();
    let generation = host
        .run("start", true, || Ok(host.advance_generation()))
        .unwrap();
    assert!(host.monitor_operation(generation).is_some());
    host.run("stop", false, || {
        host.advance_generation();
        Ok(())
    })
    .unwrap();
    assert!(host.monitor_operation(generation).is_none());
    assert!(!host.snapshot().desired_running);
    let next = host
        .run("start", true, || Ok(host.advance_generation()))
        .unwrap();
    assert!(host.monitor_operation(generation).is_none());
    assert!(host.monitor_operation(next).is_some());
}

#[test]
fn operations_serialize_and_monitor_cannot_mutate_during_start() {
    let host = Arc::new(Host::default());
    let barrier = Arc::new(Barrier::new(3));
    let active = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let (host, barrier, active) = (host.clone(), barrier.clone(), active.clone());
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            host.run("start", true, || {
                assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                assert!(host.monitor_operation(host.generation()).is_none());
                assert_eq!(host.snapshot().operation.as_deref(), Some("start"));
                std::thread::sleep(std::time::Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        }));
    }
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(host.snapshot().operation.is_none());
}

#[test]
fn failed_and_panicking_operations_leave_diagnostics_and_release_ownership() {
    let host = Host::default();
    host.run::<()>("start", true, || Err(AppError::internal("bind failed")))
        .unwrap_err();
    assert_eq!(host.snapshot().last_error.as_deref(), Some("bind failed"));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.run::<()>("stop", false, || panic!("injected"))
    }));
    assert!(host.snapshot().operation.is_none());
    assert_eq!(
        host.snapshot().last_error.as_deref(),
        Some("runtime operation panicked")
    );
    host.run("stop", false, || Ok(())).unwrap();
}

#[cfg(unix)]
#[test]
fn child_cleanup_only_stops_owned_child_and_waits_for_reap() {
    use std::process::{Command, Stdio};
    let mut external = Command::new("sh")
        .args(["-c", "read line"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut owned = Command::new("sh")
        .args(["-c", "read line"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    super::super::stop::stop_owned_child(&mut owned, std::time::Duration::from_secs(1)).unwrap();
    assert!(owned.try_wait().unwrap().is_some());
    assert!(external.try_wait().unwrap().is_none());
    external.stdin.take();
    external.wait().unwrap();
}
