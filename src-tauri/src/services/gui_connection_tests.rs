use super::*;

#[test]
fn status_poll_does_not_restore_proxy_during_a_configuration_operation() {
    let state = AppState::default();
    let _operation = state.proxy_config_operation().try_lock().unwrap();
    assert!(
        !restore_idle_proxy(&state, &CoreProcessState::Exited, || panic!(
            "must not restore during upgrade"
        ))
        .unwrap()
    );
}

#[test]
fn status_poll_does_not_restore_proxy_during_startup() {
    let state = AppState::default();
    assert!(
        !restore_idle_proxy(&state, &CoreProcessState::Starting, || panic!(
            "must not restore during startup"
        ))
        .unwrap()
    );
}

#[test]
fn idle_proxy_recovery_propagates_failure_instead_of_claiming_off() {
    let state = AppState::default();
    let error = restore_idle_proxy(&state, &CoreProcessState::Exited, || {
        Err(AppError::internal("permission denied"))
    })
    .unwrap_err();
    assert_eq!(error.message, "permission denied");
    assert!(restore_idle_proxy(&state, &CoreProcessState::Exited, || Ok(())).unwrap());
}
