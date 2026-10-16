use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Barrier,
};
fn setup(enabled: bool) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut app = AppConfig::default();
    app.release_preferences.client_auto_check = enabled;
    app_config_store::save(&path, &app).unwrap();
    let app = app_config_store::load_or_default(&path).unwrap();
    let state = AppState::new(app);
    *state.mobile_release_scheduler().directory.lock().unwrap() = Some(dir.path().into());
    state.mobile_release_scheduler().enter().unwrap();
    (dir, state)
}
fn response(tag: &str, etag: Option<&str>) -> mobile_release::Reply {
    mobile_release::Reply {
        status: 200,
        body: serde_json::to_vec(&vec![
            json!({"tag_name":tag,"draft":false,"prerelease":false,"body":"real notes"}),
        ])
        .unwrap(),
        etag: etag.map(str::to_owned),
    }
}
fn check(
    state: &AppState,
    automatic: bool,
    fetch: impl FnOnce(Option<&str>) -> AppResult<mobile_release::Reply>,
) -> AppResult<MobileReleaseCatalog> {
    state.mobile_release_scheduler().check_with(
        state,
        MobileReleaseSource::Client,
        MobileReleaseChannel::Stable,
        automatic,
        fetch,
    )
}
fn reopen(dir: &std::path::Path) -> AppState {
    let state =
        AppState::new(app_config_store::load_or_default(&dir.join("app-config.json")).unwrap());
    *state.mobile_release_scheduler().directory.lock().unwrap() = Some(dir.into());
    state.mobile_release_scheduler().enter().unwrap();
    state
}
#[test]
fn previously_saved_enabled_intent_executes_without_resave_and_disabled_does_not() {
    for enabled in [false, true] {
        let (dir, state) = setup(enabled);
        drop(state);
        let state = reopen(dir.path());
        let count = AtomicUsize::new(0);
        let result = check(&state, true, |etag| {
            assert!(etag.is_none());
            count.fetch_add(1, Ordering::SeqCst);
            Ok(response("v1.0.0", None))
        });
        assert_eq!(result.is_ok(), enabled);
        assert_eq!(count.load(Ordering::SeqCst), usize::from(enabled));
        assert!(state.configuration().transaction().is_none());
    }
}
#[test]
fn success_schedules_six_hours_failure_retries_after_fifteen_minutes_and_reopen_has_no_fake_cache()
{
    let (dir, state) = setup(true);
    let owner = state.mobile_release_scheduler();
    check(&state, true, |_| Ok(response("v1.0.0", Some("\"real\"")))).unwrap();
    let snapshot = owner.snapshot(&state).unwrap();
    let success = snapshot
        .last_success_at_unix_ms
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_eq!(
        snapshot
            .next_due_at_unix_ms
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        success + INTERVAL_MS
    );
    let (config, binding) = owner.context(&state).unwrap();
    let inner = owner.state.lock().unwrap();
    assert!(!MobileReleaseScheduler::should_check(
        &inner,
        &config,
        &binding,
        success + INTERVAL_MS - 1
    ));
    assert!(MobileReleaseScheduler::should_check(
        &inner,
        &config,
        &binding,
        success + INTERVAL_MS
    ));
    drop(inner);
    check(&state, false, |_| Err(fail("test-secret", "secret-canary"))).unwrap_err();
    let inner = owner.state.lock().unwrap();
    let attempt = inner.checkpoint.attempts.get("client:stable").unwrap();
    let retry = attempt.retry.unwrap();
    assert!(!MobileReleaseScheduler::should_check(
        &inner,
        &config,
        &binding,
        retry - 1
    ));
    assert!(MobileReleaseScheduler::should_check(
        &inner, &config, &binding, retry
    ));
    assert_eq!(attempt.success, Some(success));
    drop(inner);
    let state = reopen(dir.path());
    let snap = state.mobile_release_scheduler().snapshot(&state).unwrap();
    assert!(snap.cached_catalog.is_none());
    check(&state, false, |etag| {
        assert!(etag.is_none());
        Ok(response("v1.0.1", None))
    })
    .unwrap();
}
#[test]
fn conditional_304_reuses_only_actual_same_repository_verified_body() {
    let (_dir, state) = setup(false);
    let first = check(&state, false, |etag| {
        assert!(etag.is_none());
        Ok(response("v1.0.0", Some("\"first\"")))
    })
    .unwrap();
    let next = check(&state, false, |etag| {
        assert_eq!(etag, Some("\"first\""));
        Ok(mobile_release::Reply {
            status: 304,
            body: vec![],
            etag: None,
        })
    })
    .unwrap();
    assert_eq!(next.releases[0].version, first.releases[0].version);
    assert_eq!(next.response_status, 304);
    assert!(next.reused_validated_cache);
    let kernel = state.mobile_release_scheduler().check_with(
        &state,
        MobileReleaseSource::Kernel,
        MobileReleaseChannel::Stable,
        false,
        |etag| {
            assert!(etag.is_none());
            Ok(mobile_release::Reply {
                status: 304,
                body: vec![],
                etag: None,
            })
        },
    );
    assert_eq!(kernel.unwrap_err().code, "release_cache_missing");
}
#[test]
fn no_cache_or_no_validator_cannot_turn_304_into_a_successful_empty_catalog() {
    let (_dir, state) = setup(false);
    assert_eq!(
        check(&state, false, |_| Ok(mobile_release::Reply {
            status: 304,
            body: vec![],
            etag: None
        }))
        .unwrap_err()
        .code,
        "release_cache_missing"
    );
    check(&state, false, |_| Ok(response("v1.0.0", None))).unwrap();
    assert_eq!(
        check(&state, false, |etag| {
            assert!(etag.is_none());
            Ok(mobile_release::Reply {
                status: 304,
                body: vec![],
                etag: None,
            })
        })
        .unwrap_err()
        .code,
        "release_cache_missing"
    );
    let snap = state.mobile_release_scheduler().snapshot(&state).unwrap();
    assert_eq!(snap.cached_catalog.unwrap().releases[0].version, "1.0.0");
    assert!(!snap.cache_current);
}
#[test]
fn invalid_and_over_budget_200_keep_previous_real_directory_and_do_not_adopt_new_validator() {
    let (_dir, state) = setup(false);
    check(&state, false, |_| Ok(response("v1.0.0", Some("\"good\"")))).unwrap();
    for body in [
        b"secret-canary invalid".to_vec(),
        vec![b' '; mobile_release::MAX_BYTES + 1],
    ] {
        assert!(check(&state, false, |_| Ok(mobile_release::Reply {
            status: 200,
            body,
            etag: Some("bad".into())
        }))
        .is_err());
        let snap = state.mobile_release_scheduler().snapshot(&state).unwrap();
        assert_eq!(snap.cached_catalog.unwrap().releases[0].version, "1.0.0");
        assert!(!snap.cache_current);
    }
    check(&state, false, |etag| {
        assert_eq!(etag, Some("\"good\""));
        Ok(mobile_release::Reply {
            status: 304,
            body: vec![],
            etag: None,
        })
    })
    .unwrap();
}
#[test]
fn simultaneous_manual_and_automatic_same_binding_share_one_actual_request() {
    let (_dir, state) = setup(true);
    let state = Arc::new(state);
    let started = Arc::new(Barrier::new(2));
    let finish = Arc::new(Barrier::new(2));
    let calls = Arc::new(AtomicUsize::new(0));
    let first = {
        let state = state.clone();
        let started = started.clone();
        let finish = finish.clone();
        let calls = calls.clone();
        std::thread::spawn(move || {
            check(&state, true, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                started.wait();
                finish.wait();
                Ok(response("v1.0.0", None))
            })
        })
    };
    started.wait();
    let second = {
        let state = state.clone();
        let calls = calls.clone();
        std::thread::spawn(move || {
            check(&state, false, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(response("v9.0.0", None))
            })
        })
    };
    // The real shared-flight waiter publishes admission under the same mutex.
    // This orders completion after the second caller is waiting, with no sleep.
    let owner = state.mobile_release_scheduler();
    let mut inner = owner.state.lock().unwrap();
    while inner.waiters == 0 {
        inner = owner.complete.wait(inner).unwrap();
    }
    drop(inner);
    finish.wait();
    let a = first.join().unwrap().unwrap();
    let b = second.join().unwrap().unwrap();
    assert_eq!(a.request_id, b.request_id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn lifecycle_scope_and_preference_drift_retire_late_results_without_success_for_a_new_channel() {
    for mode in 0..3 {
        let (_dir, state) = setup(true);
        let owner = state.mobile_release_scheduler();
        let result = check(&state, true, |_| {
            match mode {
                0 => {
                    owner.leave();
                    owner.enter().unwrap();
                }
                1 => state.client_core_instance_started(),
                _ => {
                    lock(state.app_config(), "a")
                        .unwrap()
                        .release_preferences
                        .client_channel = MobileReleaseChannel::Preview;
                }
            }
            Ok(response("v1.0.0", None))
        });
        assert_eq!(result.unwrap_err().code, "release_check_invalidated");
        let snap = owner.snapshot(&state).unwrap();
        assert!(snap.last_success_at_unix_ms.is_none());
        assert!(!snap.cache_current);
    }
}
#[test]
fn failed_or_panicked_request_releases_flight_and_never_exports_raw_failure() {
    let (_dir, state) = setup(false);
    assert!(check(&state, false, |_| panic!("secret-canary")).is_err());
    assert!(
        !state
            .mobile_release_scheduler()
            .snapshot(&state)
            .unwrap()
            .checking
    );
    check(&state, false, |_| Ok(response("v1.0.0", None))).unwrap();
    assert!(
        !serde_json::to_string(&state.mobile_release_scheduler().snapshot(&state).unwrap())
            .unwrap()
            .contains("secret-canary")
    );
}
#[test]
fn preference_owned_save_requires_explicit_original_ack_and_does_not_repeat_the_file_write() {
    let (_dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    let current = lock(state.app_config(), "a").unwrap().clone();
    let id = "a".repeat(32);
    let prefs = ReleasePreferences {
        client_auto_check: true,
        ..Default::default()
    };
    let saved = owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&current).unwrap(),
            prefs.clone(),
        )
        .unwrap();
    let op = saved.operation.unwrap();
    assert_eq!(op.state, "saved-awaiting-confirm");
    assert!(!op.acknowledged);
    assert!(op.receipt.is_some());
    assert_eq!(
        check(&state, true, |_| panic!(
            "must not issue GET before acknowledgement"
        ))
        .unwrap_err()
        .code,
        "release_not_due"
    );
    let before =
        app_config_store::current_history(&owner.dir().unwrap().join("app-config.json")).unwrap();
    assert!(owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&current).unwrap(),
            prefs
        )
        .is_err());
    let confirmed = owner
        .confirm_preferences(&state, &id, op.review_binding.as_ref().unwrap())
        .unwrap();
    assert!(confirmed.operation.unwrap().acknowledged);
    assert_eq!(
        app_config_store::current_history(&owner.dir().unwrap().join("app-config.json")).unwrap(),
        before
    );
    check(&state, true, |_| Ok(response("v1.0.0", None))).unwrap();
}
#[test]
fn reopened_saved_receipt_needs_new_host_review_and_unknown_without_receipt_never_gains_one() {
    let (dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    let id = "b".repeat(32);
    let current = lock(state.app_config(), "a").unwrap().clone();
    let saved = owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&current).unwrap(),
            ReleasePreferences {
                client_auto_check: true,
                ..Default::default()
            },
        )
        .unwrap()
        .operation
        .unwrap();
    let reopened = reopen(dir.path());
    let owner = reopened.mobile_release_scheduler();
    assert!(owner
        .confirm_preferences(&reopened, &id, saved.review_binding.as_ref().unwrap())
        .is_err());
    let review = owner.operation(&reopened, &id).unwrap().unwrap();
    assert_ne!(review.review_binding, saved.review_binding);
    owner
        .confirm_preferences(&reopened, &id, review.review_binding.as_ref().unwrap())
        .unwrap();
    let mut inner = owner.state.lock().unwrap();
    let record = inner.checkpoint.operations.last_mut().unwrap();
    record.view.state = "unknown".into();
    record.view.acknowledged = false;
    record.receipt = None;
    record.view.receipt = None;
    owner.persist(&reopened, &mut inner).unwrap();
    drop(inner);
    assert!(owner
        .operation(&reopened, &id)
        .unwrap()
        .unwrap()
        .review_binding
        .is_none());
}
#[test]
fn original_preference_review_rejects_file_aba_and_scope_aba_without_rewriting() {
    for mode in 0..2 {
        let (_dir, state) = setup(false);
        let owner = state.mobile_release_scheduler();
        let current = lock(state.app_config(), "a").unwrap().clone();
        let id = "c".repeat(32);
        let op = owner
            .save_preferences(
                &state,
                &id,
                &mobile_release::revision(&current).unwrap(),
                ReleasePreferences {
                    client_auto_check: true,
                    ..Default::default()
                },
            )
            .unwrap()
            .operation
            .unwrap();
        if mode == 0 {
            let app = lock(state.app_config(), "a").unwrap().clone();
            let path = owner.dir().unwrap().join("app-config.json");
            let mut changed = app.clone();
            changed.ui.theme = "dark".into();
            app_config_store::save(&path, &changed).unwrap();
            app_config_store::save(&path, &app).unwrap();
        } else {
            state.client_core_instance_started();
            state.client_core_instance_lost();
        }
        assert!(owner
            .confirm_preferences(&state, &id, op.review_binding.as_ref().unwrap())
            .is_err());
        assert!(!owner.operation(&state, &id).unwrap().unwrap().acknowledged);
    }
}
#[test]
fn native_worker_admission_deduplicates_resume_and_due_metadata_contains_no_catalog_body_or_secrets(
) {
    let (_dir, state) = setup(true);
    let owner = state.mobile_release_scheduler();
    assert!(!owner.enter().unwrap());
    owner.leave();
    assert!(!owner.enter().unwrap());
    check(&state, false, |_| {
        Ok(response("v1.0.0", Some("private-validator-canary")))
    })
    .unwrap();
    let inner = owner.state.lock().unwrap();
    let bytes = serde_json::to_vec(&inner.checkpoint).unwrap();
    assert!(bytes.len() < 65536);
    let text = String::from_utf8(bytes).unwrap();
    for secret in [
        "real notes",
        "private-validator-canary",
        "github.com",
        "tag_name",
    ] {
        assert!(!text.contains(secret));
    }
}

#[test]
fn real_http_stream_executor_sends_original_etag_and_reuses_body_only_on_304() {
    use std::io::{Read, Write};
    let (_dir, state) = setup(false);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("bounded fixture accept failed: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 1024];
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() <= 8192);
            }
            requests.push(String::from_utf8(bytes).unwrap().to_ascii_lowercase());
            if index == 0 {
                let body = response("v1.2.3", None).body;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"actual-wire-validator\"\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 304 Not Modified\r\nETag: \"actual-wire-validator\"\r\nConnection: close\r\n\r\n").unwrap();
            }
        }
        requests
    });
    let first = check(&state, false, |etag| {
        mobile_release::fetch_for_test(state.capabilities(), url.clone(), etag)
    })
    .unwrap();
    let second = check(&state, false, |etag| {
        mobile_release::fetch_for_test(state.capabilities(), url.clone(), etag)
    })
    .unwrap();
    let requests = server.join().unwrap();
    assert!(!requests[0].contains("if-none-match:"));
    assert!(requests[1].contains("if-none-match: \"actual-wire-validator\"\r\n"));
    assert_eq!(first.releases[0].version, "1.2.3");
    assert_eq!(second.releases[0].version, "1.2.3");
    assert_eq!(second.response_status, 304);
    assert!(second.reused_validated_cache);
}
#[test]
fn empty_validator_and_new_200_without_validator_cannot_authorize_later_304() {
    let (_dir, state) = setup(false);
    check(&state, false, |_| Ok(response("v1.0.0", Some("")))).unwrap();
    assert!(check(&state, false, |etag| {
        assert!(etag.is_none());
        Ok(mobile_release::Reply {
            status: 304,
            body: vec![],
            etag: None,
        })
    })
    .is_err());
    check(&state, false, |_| Ok(response("v1.0.1", Some("good")))).unwrap();
    check(&state, false, |_| Ok(response("v1.0.2", None))).unwrap();
    assert!(check(&state, false, |etag| {
        assert!(etag.is_none());
        Ok(mobile_release::Reply {
            status: 304,
            body: vec![],
            etag: None,
        })
    })
    .is_err());
}
#[test]
fn damaged_checkpoint_pauses_automatic_checks_but_not_explicit_manual_get() {
    let (dir, state) = setup(true);
    let mut revision = None;
    schedule_store::save_at(
        dir.path(),
        state.capabilities(),
        STORE,
        &json!({"unexpected":"secret-canary"}),
        &mut revision,
    )
    .unwrap();
    let reopened = reopen(dir.path());
    let owner = reopened.mobile_release_scheduler();
    let snapshot = owner.snapshot(&reopened).unwrap();
    assert!(snapshot.storage_error);
    assert_eq!(snapshot.outcome, "storage-error");
    assert!(check(&reopened, true, |_| panic!("automatic IO must not start")).is_err());
    let actual = check(&reopened, false, |_| Ok(response("v1.0.0", None))).unwrap();
    assert_eq!(actual.releases[0].version, "1.0.0");
    assert!(owner.snapshot(&reopened).unwrap().storage_error);
}
#[test]
fn malformed_or_forged_preference_metadata_never_grants_auto_or_fresh_review() {
    let (dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    let id = "d".repeat(32);
    let app = lock(state.app_config(), "a").unwrap().clone();
    owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&app).unwrap(),
            ReleasePreferences {
                client_auto_check: true,
                ..Default::default()
            },
        )
        .unwrap();
    let inner = owner.state.lock().unwrap();
    let pristine = inner.checkpoint.clone();
    let mut revision = inner.store_revision;
    drop(inner);
    for case in 0..6 {
        let mut damaged = pristine.clone();
        let record = damaged.operations.last_mut().unwrap();
        match case {
            0 => {
                record.view.state = "confirmed".into();
                record.view.acknowledged = true;
                record.view.receipt = None;
                record.receipt = None;
            }
            1 => record.view.receipt.as_mut().unwrap().owner_operation = "e".repeat(32),
            2 => record.view.receipt.as_mut().unwrap().after_epoch = "18446744073709551615".into(),
            3 => record.receipt.as_mut().unwrap().after.store_id = "f".repeat(32),
            4 => record.view.acknowledged = true,
            _ => {
                let extra = record.clone();
                damaged.operations.push(extra);
            }
        }
        assert!(!validate(&damaged));
        schedule_store::save_at(
            dir.path(),
            state.capabilities(),
            STORE,
            &damaged,
            &mut revision,
        )
        .unwrap();
        let reopened = reopen(dir.path());
        assert!(
            reopened
                .mobile_release_scheduler()
                .snapshot(&reopened)
                .unwrap()
                .storage_error
        );
        assert!(reopened
            .mobile_release_scheduler()
            .operation(&reopened, &id)
            .is_err());
        assert!(check(&reopened, true, |_| panic!(
            "forged receipt must not grant auto"
        ))
        .is_err());
    }
}
#[test]
fn explicit_cas_rejection_after_prepared_checkpoint_is_not_an_unknown_file_write() {
    let (_dir, state) = setup(false);
    let state = Arc::new(state);
    let owner = state.mobile_release_scheduler();
    let current = lock(state.app_config(), "a").unwrap().clone();
    let id = "e".repeat(32);
    let other = state.clone();
    let path = owner.dir().unwrap().join("app-config.json");
    *owner.before_file.lock().unwrap() = Some(Box::new(move || {
        let mut app = lock(other.app_config(), "a").unwrap();
        app.ui.theme = "dark".into();
        app_config_store::save(&path, &app).unwrap();
    }));
    assert_eq!(
        owner
            .save_preferences(
                &state,
                &id,
                &mobile_release::revision(&current).unwrap(),
                ReleasePreferences {
                    client_auto_check: true,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        "release_preferences_rejected"
    );
    let op = owner.operation(&state, &id).unwrap().unwrap();
    assert_eq!(op.state, "rejected");
    assert!(op.receipt.is_none());
    assert!(op.review_binding.is_none());
    assert!(
        !lock(state.app_config(), "a")
            .unwrap()
            .release_preferences
            .client_auto_check
    );
    assert_eq!(
        app_config_store::load_or_default(&owner.dir().unwrap().join("app-config.json"))
            .unwrap()
            .ui
            .theme,
        "dark"
    );
}
#[test]
fn receipt_checkpoint_failure_after_actual_file_write_reopens_as_unconfirmed_without_replay() {
    let (dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    owner.snapshot(&state).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("znet-sink.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_release_receipt BEFORE UPDATE ON capability_values WHEN instr(CAST(NEW.value AS TEXT),'saved-awaiting-confirm')>0 BEGIN SELECT RAISE(ABORT,'injected fault'); END;").unwrap();
    let current = lock(state.app_config(), "a").unwrap().clone();
    let id = "f".repeat(32);
    assert_eq!(
        owner
            .save_preferences(
                &state,
                &id,
                &mobile_release::revision(&current).unwrap(),
                ReleasePreferences {
                    client_auto_check: true,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        "release_preferences_unknown"
    );
    assert!(
        app_config_store::load_or_default(&dir.path().join("app-config.json"))
            .unwrap()
            .release_preferences
            .client_auto_check
    );
    db.execute_batch("DROP TRIGGER reject_release_receipt")
        .unwrap();
    drop(db);
    let reopened = reopen(dir.path());
    let owner = reopened.mobile_release_scheduler();
    let op = owner.operation(&reopened, &id).unwrap().unwrap();
    assert_eq!(op.state, "prepared");
    assert!(op.receipt.is_none());
    assert!(op.review_binding.is_none());
    assert!(check(&reopened, true, |_| panic!(
        "prepared without receipt must not authorize GET"
    ))
    .is_err());
    let before = app_config_store::current_history(&dir.path().join("app-config.json")).unwrap();
    assert!(owner
        .save_preferences(&reopened, &id, &op.expected_revision, op.preferences)
        .is_err());
    assert_eq!(
        before,
        app_config_store::current_history(&dir.path().join("app-config.json")).unwrap()
    );
}

#[test]
fn acknowledgement_checkpoint_failure_is_exposed_as_unknown_and_reopen_requires_fresh_review() {
    let (dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    let current = lock(state.app_config(), "a").unwrap().clone();
    let id = "1".repeat(32);
    let operation = owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&current).unwrap(),
            ReleasePreferences {
                client_auto_check: true,
                ..Default::default()
            },
        )
        .unwrap()
        .operation
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("znet-sink.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_release_ack BEFORE UPDATE ON capability_values WHEN instr(CAST(NEW.value AS TEXT),'\"acknowledged\":true')>0 BEGIN SELECT RAISE(ABORT,'injected fault'); END;").unwrap();
    assert!(owner
        .confirm_preferences(&state, &id, operation.review_binding.as_ref().unwrap())
        .is_err());
    let snapshot = owner.snapshot(&state).unwrap();
    assert!(snapshot.storage_error);
    let shown = snapshot.preference_operation.unwrap();
    assert_eq!(shown.state, "unknown");
    assert!(!shown.acknowledged);
    assert!(shown.review_binding.is_none());
    assert!(shown.receipt.is_some());
    assert_eq!(
        owner.preferences(&state).unwrap().operation.unwrap().state,
        "unknown"
    );
    db.execute_batch("DROP TRIGGER reject_release_ack").unwrap();
    drop(db);
    let reopened = reopen(dir.path());
    let owner = reopened.mobile_release_scheduler();
    let original = owner.operation(&reopened, &id).unwrap().unwrap();
    assert_eq!(original.state, "saved-awaiting-confirm");
    assert!(!original.acknowledged);
    assert!(original.review_binding.is_some());
    owner
        .confirm_preferences(&reopened, &id, original.review_binding.as_ref().unwrap())
        .unwrap();
}

#[test]
fn original_preference_review_rejects_foreground_lifecycle_aba() {
    let (_dir, state) = setup(false);
    let owner = state.mobile_release_scheduler();
    let app = lock(state.app_config(), "a").unwrap().clone();
    let id = "2".repeat(32);
    let operation = owner
        .save_preferences(
            &state,
            &id,
            &mobile_release::revision(&app).unwrap(),
            ReleasePreferences {
                client_auto_check: true,
                ..Default::default()
            },
        )
        .unwrap()
        .operation
        .unwrap();
    let core = state.client_core_snapshot();
    let history =
        app_config_store::current_history(&owner.dir().unwrap().join("app-config.json")).unwrap();
    owner.leave();
    owner.enter().unwrap();
    assert_eq!(core.scope, state.client_core_snapshot().scope);
    assert_eq!(
        core.scope_generation,
        state.client_core_snapshot().scope_generation
    );
    assert_eq!(
        owner
            .confirm_preferences(&state, &id, operation.review_binding.as_ref().unwrap())
            .unwrap_err()
            .code,
        "release_check_invalidated"
    );
    assert_eq!(
        history,
        app_config_store::current_history(&owner.dir().unwrap().join("app-config.json")).unwrap()
    );
    assert!(!owner.operation(&state, &id).unwrap().unwrap().acknowledged);
}
