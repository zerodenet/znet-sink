use super::*;
use crate::errors::AppError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};

#[test]
fn concurrent_pollers_share_one_os_query() {
    let cache = Arc::new(StatusCache::<usize>::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let (cache, calls, barrier) = (cache.clone(), calls.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                cache
                    .get(false, || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(Duration::from_millis(20));
                        Ok(42)
                    })
                    .unwrap()
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().unwrap(), 42);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn fresh_reads_invalidation_and_expiry_never_keep_an_old_snapshot() {
    let cache = StatusCache::new();
    assert_eq!(cache.get(false, || Ok(1)).unwrap(), 1);
    assert_eq!(cache.get(false, || Ok(2)).unwrap(), 1);
    assert_eq!(cache.get(true, || Ok(3)).unwrap(), 3);
    cache.invalidate();
    assert_eq!(cache.get(false, || Ok(4)).unwrap(), 4);
    cache.entry.lock().unwrap().as_mut().unwrap().0 = Instant::now() - Duration::from_secs(6);
    assert_eq!(cache.get(false, || Ok(5)).unwrap(), 5);
}

#[test]
fn errors_are_coalesced_briefly_and_not_reported_as_disabled_success() {
    let cache = StatusCache::<usize>::new();
    assert!(cache
        .get(false, || Err(AppError::internal("OS query failed")))
        .is_err());
    assert!(cache.get(false, || Ok(42)).is_err());
    cache.entry.lock().unwrap().as_mut().unwrap().0 = Instant::now() - Duration::from_secs(2);
    assert_eq!(cache.get(false, || Ok(42)).unwrap(), 42);
}
