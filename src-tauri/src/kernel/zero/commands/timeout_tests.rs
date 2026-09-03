use super::{command_response_timeout, TUN_RESPONSE_TIMEOUT};
use crate::kernel::protocol::{response_timeout_from_options, timeout_from_options};
use crate::models::core::CoreIpcOptions;
use std::time::Duration;

#[test]
fn tun_commands_share_a_long_response_budget_without_delaying_connection_or_queries() {
    for method in ["tun.start", "tun.stop"] {
        let options = CoreIpcOptions {
            socket: Some("isolated-test-pipe".to_string()),
            timeout_ms: Some(2_000),
        };
        assert_eq!(
            response_timeout_from_options(Some(&options), command_response_timeout(method))
                .unwrap(),
            Duration::from_secs(15)
        );
        assert_eq!(
            timeout_from_options(Some(&options)).unwrap(),
            Duration::from_secs(2)
        );
        assert_eq!(options.socket.as_deref(), Some("isolated-test-pipe"));
        assert_eq!(
            response_timeout_from_options(None, command_response_timeout(method)).unwrap(),
            TUN_RESPONSE_TIMEOUT
        );
    }
    for method in [
        "config.apply",
        "policies.select",
        "diagnostics.probe_outbound",
        "tun.status",
    ] {
        assert_eq!(command_response_timeout(method), None);
        assert_eq!(
            response_timeout_from_options(None, None).unwrap(),
            Duration::from_secs(2)
        );
    }
}

#[test]
fn tun_budget_preserves_longer_explicit_timeouts_and_rejects_invalid_options() {
    let minimum = command_response_timeout("tun.start");
    let options = CoreIpcOptions {
        socket: None,
        timeout_ms: Some(30_000),
    };
    assert_eq!(
        response_timeout_from_options(Some(&options), minimum).unwrap(),
        Duration::from_secs(30)
    );
    for timeout_ms in [0, crate::config::MAX_IPC_TIMEOUT_MS + 1] {
        let options = CoreIpcOptions {
            socket: None,
            timeout_ms: Some(timeout_ms),
        };
        assert_eq!(
            response_timeout_from_options(Some(&options), minimum)
                .unwrap_err()
                .code,
            "invalid_argument"
        );
    }
}

#[test]
fn five_second_tun_response_succeeds_but_missing_response_still_times_out() {
    // Virtual time + an in-memory response channel only. No kernel, network,
    // device, privileges or real five-second sleep is needed for this test.
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            for method in ["tun.start", "tun.stop"] {
                let budget =
                    response_timeout_from_options(None, command_response_timeout(method)).unwrap();
                let (tx, rx) = tokio::sync::oneshot::channel();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    tx.send("completed").unwrap();
                });
                assert_eq!(
                    tokio::time::timeout(budget, rx).await.unwrap().unwrap(),
                    "completed"
                );

                let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
                let start = tokio::time::Instant::now();
                assert!(tokio::time::timeout(budget, rx).await.is_err());
                assert_eq!(start.elapsed(), budget);
            }
            // The same delayed reply still exceeds an ordinary query budget.
            assert!(tokio::time::timeout(
                response_timeout_from_options(None, None).unwrap(),
                tokio::time::sleep(Duration::from_secs(5)),
            )
            .await
            .is_err());
        });
}
