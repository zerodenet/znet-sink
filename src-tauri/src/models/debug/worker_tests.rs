use super::*;
use std::time::Duration;

#[test]
fn stalled_diagnostic_storage_never_blocks_delivery_and_reports_bounded_loss() {
    let (entered, ready) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let (done, results) = mpsc::channel();
    let worker = Worker::new(1, move |item, dropped| {
        if item == 1 {
            entered.send(()).unwrap();
            blocked.recv().unwrap();
        }
        done.send((item, dropped)).unwrap();
    })
    .unwrap();
    worker.submit(1).unwrap();
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    // Storage is still blocked: enqueueing must return now, and overflow must
    // be counted rather than waiting for the reader or growing the queue.
    worker.submit(2).unwrap();
    assert_eq!(worker.submit(3), Err(3));
    release.send(()).unwrap();
    assert_eq!(
        results.recv_timeout(Duration::from_secs(1)).unwrap(),
        (1, 0)
    );
    assert_eq!(
        results.recv_timeout(Duration::from_secs(1)).unwrap(),
        (2, 1)
    );
    worker.submit(4).unwrap();
    assert_eq!(
        results.recv_timeout(Duration::from_secs(1)).unwrap(),
        (4, 0)
    );
}

#[test]
fn diagnostic_failure_does_not_kill_following_delivery() {
    let (done, results) = mpsc::channel();
    let worker = Worker::new(2, move |item, dropped| {
        assert_ne!(item, 1, "simulated diagnostic sink failure");
        done.send((item, dropped)).unwrap();
    })
    .unwrap();
    worker.submit(1).unwrap();
    worker.submit(2).unwrap();
    assert_eq!(
        results.recv_timeout(Duration::from_secs(1)).unwrap(),
        (2, 1)
    );
}
