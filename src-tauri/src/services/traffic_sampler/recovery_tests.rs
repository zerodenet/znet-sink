use super::*;

#[test]
fn fresh_pushes_avoid_queries_but_silence_recovers_at_a_bounded_rate() {
    let start = Instant::now();
    let mut recovery = Recovery::new(start);
    assert!(!recovery.due(start + Duration::from_secs(2)));
    assert!(recovery.due(start + Duration::from_secs(3)));
    assert!(!recovery.due(start + Duration::from_millis(3500)));
    assert!(recovery.due(start + Duration::from_secs(4)));
    recovery.pushed(start + Duration::from_secs(4), 4000, 4000);
    assert!(!recovery.due(start + Duration::from_secs(5)));
}

#[test]
fn old_backlogged_pushes_do_not_disguise_a_stalled_stream() {
    let start = Instant::now();
    let mut recovery = Recovery::new(start);
    recovery.pushed(start + Duration::from_secs(5), 1000, 6000);
    assert!(recovery.due(start + Duration::from_secs(5)));
    assert!(recovery.due(start + Duration::from_secs(6)));
}
