use super::*;
use std::cell::Cell;
use std::future::ready;

fn default_restoration_timeout() -> Duration {
    super::super::tun::restoration_timeout(&crate::models::core::CoreIpcOptions {
        socket: None,
        timeout_ms: None,
    })
    .unwrap()
}

#[test]
fn restoration_deadline_covers_the_command_and_preserves_longer_ipc_budgets() {
    use crate::kernel::{protocol, zero::commands};
    use crate::models::core::CoreIpcOptions;
    for timeout_ms in [None, Some(90_000)] {
        let options = CoreIpcOptions {
            socket: None,
            timeout_ms,
        };
        let response = protocol::response_timeout_from_options(
            Some(&options),
            commands::command_response_timeout("tun.start"),
        )
        .unwrap();
        let restoration = super::super::tun::restoration_timeout(&options).unwrap();
        assert!(
            restoration > response + protocol::timeout_from_options(Some(&options)).unwrap() * 3
        );
    }
    for timeout_ms in [0, crate::config::MAX_IPC_TIMEOUT_MS + 1] {
        assert_eq!(
            super::super::tun::restoration_timeout(&CoreIpcOptions {
                socket: None,
                timeout_ms: Some(timeout_ms),
            })
            .unwrap_err()
            .code,
            "invalid_argument"
        );
    }
}

#[test]
fn slow_activation_and_lost_reply_recovery_fit_the_real_restoration_budget() {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            let calls = Cell::new(0);
            restore(
                || ready(Ok(status(false))),
                || {
                    calls.set(calls.get() + 1);
                    async {
                        tokio::time::sleep(Duration::from_secs(20)).await;
                        Ok(status(true))
                    }
                },
                default_restoration_timeout(),
                Duration::from_millis(100),
            )
            .await
            .unwrap();
            assert_eq!(calls.get(), 1);

            let started = tokio::time::Instant::now();
            let calls = Cell::new(0);
            restore(
                || ready(Ok(status(started.elapsed() >= Duration::from_secs(65)))),
                || {
                    calls.set(calls.get() + 1);
                    async {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        Err(timeout())
                    }
                },
                default_restoration_timeout(),
                Duration::from_millis(100),
            )
            .await
            .unwrap();
            assert_eq!(
                calls.get(),
                1,
                "lost response must only trigger observation, not a second start"
            );
        });
}

#[test]
fn expired_restoration_reports_ended_wait_and_residual_authorization_prompt() {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            let budget = default_restoration_timeout();
            let started = tokio::time::Instant::now();
            let calls = Cell::new(0);
            let error = restore(
                || ready(Ok(status(false))),
                || {
                    calls.set(calls.get() + 1);
                    std::future::pending::<AppResult<GuiTunStatus>>()
                },
                budget,
                Duration::from_millis(100),
            )
            .await
            .unwrap_err();
            assert_eq!(started.elapsed(), budget);
            assert_eq!(calls.get(), 1);
            assert_eq!(error.code, "tun_restore_unconfirmed");
            assert!(error.message.contains("本次等待已结束"));
            assert!(error.message.contains("系统授权窗口"));
            assert_eq!(
                error.details.unwrap()["timeoutMs"],
                budget.as_millis() as u64
            );
        });
}

fn status(enabled: bool) -> GuiTunStatus {
    GuiTunStatus {
        enabled,
        healthy: enabled,
        supported: true,
        ..Default::default()
    }
}

fn timeout() -> AppError {
    AppError {
        code: "timeout",
        message: "lost reply".into(),
        details: None,
    }
}

#[tokio::test]
async fn enabled_but_unhealthy_tun_is_never_reported_as_restored() {
    let starts = Cell::new(0);
    let result = restore(
        || {
            ready(Ok(GuiTunStatus {
                healthy: false,
                ..status(true)
            }))
        },
        || {
            starts.set(starts.get() + 1);
            ready(Ok(status(true)))
        },
        Duration::ZERO,
        Duration::ZERO,
    )
    .await;
    assert_eq!(result.unwrap_err().code, "tun_restore_unconfirmed");
    assert_eq!(starts.get(), 0);
}

#[tokio::test]
async fn late_start_is_confirmed_without_resubmitting_mutation() {
    let queries = Cell::new(0);
    let starts = Cell::new(0);
    restore(
        || {
            queries.set(queries.get() + 1);
            ready(Ok(status(queries.get() >= 3)))
        },
        || {
            starts.set(starts.get() + 1);
            ready(Err(timeout()))
        },
        Duration::from_secs(1),
        Duration::ZERO,
    )
    .await
    .unwrap();
    assert_eq!(starts.get(), 1);
    assert_eq!(queries.get(), 3);
}

#[tokio::test]
async fn unknown_state_does_not_start_tun_and_reports_failure() {
    let starts = Cell::new(0);
    let result = restore(
        || ready(Err(timeout())),
        || {
            starts.set(starts.get() + 1);
            ready(Ok(status(true)))
        },
        Duration::ZERO,
        Duration::ZERO,
    )
    .await;
    assert_eq!(result.unwrap_err().code, "timeout");
    assert_eq!(starts.get(), 0);
}

#[tokio::test]
async fn stopped_acknowledgement_is_not_successful_restoration() {
    let result = restore(
        || ready(Ok(status(false))),
        || ready(Ok(status(false))),
        Duration::ZERO,
        Duration::ZERO,
    )
    .await;
    assert_eq!(result.unwrap_err().code, "tun_restore_unconfirmed");
}

#[tokio::test]
async fn restart_of_stopped_kernel_does_not_query_a_missing_runtime() {
    assert!(!super::restart_intent(None, false, || async {
        panic!("stopped kernel has no IPC")
    })
    .await
    .unwrap());
    assert!(super::restart_intent(Some(true), false, || async {
        panic!("saved intent is authoritative")
    })
    .await
    .unwrap());
}

#[tokio::test]
async fn restart_preserves_legacy_live_tun_and_fails_before_stopping_if_unknown() {
    assert!(super::restart_intent(None, true, || async { Ok(true) })
        .await
        .unwrap());
    assert!(super::restart_intent(None, true, || async {
        Err(crate::errors::AppError::internal("cannot inspect TUN"))
    })
    .await
    .is_err());
    assert!(!super::restart_intent(Some(false), true, || async {
        panic!("explicit disabled intent is authoritative")
    })
    .await
    .unwrap());
}

#[tokio::test]
async fn stalled_status_query_has_a_bounded_deadline_and_does_not_enable() {
    let error = restore(
        || std::future::pending::<AppResult<GuiTunStatus>>(),
        || async { panic!("must not enable without status") },
        Duration::from_millis(10),
        Duration::from_millis(1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "tun_restore_unconfirmed");
}

#[tokio::test]
async fn stalled_enable_is_submitted_once_and_remains_unconfirmed() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let error = restore(
        || async { Ok(status(false)) },
        || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::pending::<AppResult<GuiTunStatus>>()
        },
        Duration::from_millis(10),
        Duration::from_millis(1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "tun_restore_unconfirmed");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
