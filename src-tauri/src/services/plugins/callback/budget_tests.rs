use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

struct Cleanup(Store);
impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.sessions.lock().unwrap().clear();
    }
}

#[test]
fn callback_session_limits_bound_live_threads_and_retained_results() {
    let store = Store::default();
    let _cleanup = Cleanup(store.clone());
    let owner = Scope::fixture("org.example/main");
    let mut owned = Vec::new();
    for _ in 0..MAX_OWNER_SESSIONS {
        owned.push(store.create(owner.clone(), &|| Ok(())).unwrap());
    }
    let error = match store.create(owner.clone(), &|| Ok(())) {
        Err(error) => error,
        Ok(_) => panic!("callback quota was not enforced"),
    };
    assert_eq!(error.code, "plugin_callback_budget");
    for index in MAX_OWNER_SESSIONS..MAX_SESSIONS {
        store
            .create(Scope::fixture(&format!("org.other/owner{index}")), &|| {
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(store.sessions.lock().unwrap().len(), MAX_SESSIONS);
    assert!(store
        .create(Scope::fixture("org.last/main"), &|| Ok(()))
        .is_err());
    assert!(store
        .cancel(
            &Scope::fixture("org.other/main"),
            &owned[0].session_id,
            &|| Ok(())
        )
        .is_err());
    assert_eq!(store.sessions.lock().unwrap().len(), MAX_SESSIONS);
    store
        .cancel(&owner, &owned[0].session_id, &|| Ok(()))
        .unwrap();
    store
        .create(Scope::fixture("org.last/main"), &|| Ok(()))
        .unwrap();
    assert_eq!(store.sessions.lock().unwrap().len(), MAX_SESSIONS);
}

#[test]
fn monotonic_expiry_removes_sensitive_results_even_if_displayed_wall_time_is_future() {
    let store = Store::default();
    let _cleanup = Cleanup(store.clone());
    let scope = Scope::fixture("org.example/main");
    let created = store.create(scope.clone(), &|| Ok(())).unwrap();
    assert!(created.expires_at_unix_ms > crate::services::common::now_unix_ms());
    {
        let mut sessions = store.sessions.lock().unwrap();
        let session = sessions.get_mut(&created.session_id).unwrap();
        session.state =
            State::Completed(BTreeMap::from([("code".into(), "isolated-fixture".into())]));
        session.deadline = Instant::now() - Duration::from_millis(1);
    }
    assert_eq!(
        store.poll(&scope, &created.session_id).unwrap(),
        serde_json::json!({"state":"expired"})
    );
    assert!(matches!(
        &store.sessions.lock().unwrap()[&created.session_id].state,
        State::Expired
    ));
    store.create(scope.clone(), &|| Ok(())).unwrap();
    assert!(store.poll(&scope, &created.session_id).is_err());
    assert_eq!(store.sessions.lock().unwrap().len(), 1);
}

#[test]
fn trickled_headers_do_not_extend_the_original_total_read_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .write_all(b"GET /callback/fixed?code=opaque HTTP/1.1\r\nX-Pending: ")
        .unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let drip_stop = stop.clone();
    let writer = thread::spawn(move || {
        while !drip_stop.load(Ordering::Acquire) {
            if client.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        let _ = client.shutdown(std::net::Shutdown::Both);
    });
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        sent.send(read_callback_until(
            &mut stream,
            "fixed",
            Instant::now() + Duration::from_millis(200),
        ))
        .unwrap();
    });
    let result = received.recv_timeout(Duration::from_secs(2));
    // Cleanup happens before the assertion, including on a regression failure.
    stop.store(true, Ordering::Release);
    writer.join().unwrap();
    reader.join().unwrap();
    assert!(
        matches!(result, Ok(Err(()))),
        "a trickled header extended the absolute read budget"
    );
}

#[test]
fn invalid_session_ids_and_missing_ids_do_not_reflect_untrusted_input() {
    let store = Store::default();
    let scope = Scope::fixture("org.example/main");
    for id in ["https://private.invalid/?token=fixture", "", "../outside"] {
        let error = store.poll(&scope, id).unwrap_err();
        assert_eq!(error.message, "浏览器回调会话编号无效");
    }
    let error = store.poll(&scope, &"a".repeat(48)).unwrap_err();
    assert!(!error.message.contains(&"a".repeat(48)));
}

#[test]
fn public_callback_errors_keep_fixed_reasons_without_internal_causes() {
    for (code, expected) in [
        (
            "plugin_callback_budget",
            znet_plugin_sandbox::sdk::ErrorCode::BudgetExceeded,
        ),
        (
            "plugin_callback_unavailable",
            znet_plugin_sandbox::sdk::ErrorCode::Transport,
        ),
    ] {
        let public = super::super::sdk::public_failure(AppError {
            code,
            message: "https://private.invalid/?token=isolated-fixture".into(),
            details: None,
        });
        assert_eq!(public.code, expected);
        assert!(!public.message.contains("private.invalid"));
        assert!(public.diagnostics.is_empty());
        assert!(public.field_path.is_none());
    }
}
