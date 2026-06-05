use crate::models::gui_core::GuiTrafficStats;
use crate::kernel::zero::adapter::*;

#[test]
fn traffic_rates_use_byte_delta_over_interval() {
    let previous = GuiTrafficStats {
        bytes_up: 1_000,
        bytes_down: 10_000,
        ..GuiTrafficStats::default()
    };
    let current = GuiTrafficStats {
        bytes_up: 3_000,
        bytes_down: 15_000,
        ..GuiTrafficStats::default()
    };

    let rates = calculate_rates(&previous, &current, 2_000);

    assert_eq!(rates.upload_bps, 1_000);
    assert_eq!(rates.download_bps, 2_500);
}

#[test]
fn traffic_rates_treat_counter_reset_as_zero_rate() {
    assert_eq!(bytes_delta_per_second(10_000, 5_000, 1_000), 0);
}

#[test]
fn first_traffic_snapshot_is_unstable_baseline() {
    let snapshot = build_traffic_snapshot(
        GuiTrafficStats {
            bytes_up: 100,
            bytes_down: 200,
            ..GuiTrafficStats::default()
        },
        None,
        1_000,
    );

    assert!(!snapshot.stable);
    assert_eq!(snapshot.rates.upload_bps, 0);
    assert!(snapshot.interval_ms.is_none());
}

#[test]
fn second_traffic_snapshot_reports_rates() {
    let previous = TrafficSample {
        stats: GuiTrafficStats {
            bytes_up: 100,
            bytes_down: 200,
            ..GuiTrafficStats::default()
        },
        sampled_at_unix_ms: 1_000,
    };

    let snapshot = build_traffic_snapshot(
        GuiTrafficStats {
            bytes_up: 1_100,
            bytes_down: 2_200,
            ..GuiTrafficStats::default()
        },
        Some(&previous),
        2_000,
    );

    assert!(snapshot.stable);
    assert_eq!(snapshot.interval_ms, Some(1_000));
    assert_eq!(snapshot.rates.upload_bps, 1_000);
    assert_eq!(snapshot.rates.download_bps, 2_000);
}

#[test]
fn snapshot_with_short_interval_is_unstable() {
    let previous = TrafficSample {
        stats: GuiTrafficStats {
            bytes_up: 100,
            bytes_down: 200,
            ..GuiTrafficStats::default()
        },
        sampled_at_unix_ms: 1_000,
    };

    let snapshot = build_traffic_snapshot(
        GuiTrafficStats {
            bytes_up: 200,
            bytes_down: 300,
            ..GuiTrafficStats::default()
        },
        Some(&previous),
        1_200, // 200ms interval, below 500ms threshold
    );

    assert!(!snapshot.stable);
}
