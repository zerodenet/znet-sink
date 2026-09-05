use super::*;
use std::cell::Cell;
use std::future::ready;

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
