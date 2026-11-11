use super::*;

#[test]
fn consecutive_failures_exhaust_the_budget_including_failed_spawns() {
    let start = Instant::now();
    let mut budget = RestartBudget::new(5, Duration::from_secs(60));
    for attempt in 1..=5 {
        assert!(budget.allow_next(start + Duration::from_secs(attempt * 100)));
        assert_eq!(budget.attempt(), attempt as u32);
    }
    assert!(!budget.allow_next(start + Duration::from_secs(1000)));
}

#[test]
fn slow_tun_restoration_never_counts_as_stable_running_time() {
    let start = Instant::now();
    let mut budget = RestartBudget::new(2, Duration::from_secs(60));
    assert!(budget.allow_next(start));
    budget.mark_ready(start + Duration::from_secs(75));
    assert!(budget.allow_next(start + Duration::from_secs(76)));
    budget.mark_ready(start + Duration::from_secs(151));
    assert!(!budget.allow_next(start + Duration::from_secs(152)));
}

#[test]
fn a_confirmed_stable_run_resets_the_budget_once_not_on_every_retry() {
    let start = Instant::now();
    let mut budget = RestartBudget::new(2, Duration::from_secs(60));
    assert!(budget.allow_next(start));
    budget.mark_ready(start);
    assert!(budget.allow_next(start + Duration::from_secs(61)));
    assert_eq!(budget.attempt(), 1);
    assert!(budget.allow_next(start + Duration::from_secs(200)));
    assert!(!budget.allow_next(start + Duration::from_secs(400)));
}
