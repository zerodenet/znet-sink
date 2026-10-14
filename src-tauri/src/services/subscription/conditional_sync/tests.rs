use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};
fn fixture() -> (SubscriptionProfile, ProxyConfigProfile) {
    let sub = serde_json::from_value(json!({"id":"ordinary","name":"Source","url":"https://example.invalid/sub?token=secret-canary","enabled":true,"kernel":"zero","format":"auto","userAgent":"custom-canary","targetProxyConfigId":"material","updatedAtUnixMs":10,"lastSyncAtUnixMs":10,"lastError":null,"nodeCount":1,"uploadBytes":5,"policySelections":{"group":"direct"}})).unwrap();
    let profile = ProxyConfigProfile {
        id: "material".into(),
        name: "material".into(),
        kernel: "zero".into(),
        format: "zero-json".into(),
        path: None,
        content: Some(
            json!({"inbounds":[],"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],"route":{"final":{"type":"direct"}}}),
        ),
        active: false,
        managed_source: None,
        updated_at_unix_ms: 8,
        capabilities: Default::default(),
    };
    (sub, profile)
}
fn validators() -> Validators {
    Validators {
        etag: Some("\"v1\"".into()),
        last_modified: Some("Mon, 12 Oct 2026 01:00:00 GMT".into()),
    }
}
#[test]
fn exact_identity_rejects_source_material_and_unknown_result_changes() {
    let (sub, profile) = fixture();
    let cache = Cache::confirmed(&sub, "custom", &profile, &validators()).unwrap();
    assert!(cache.matches(&sub, "custom", &profile));
    for field in [
        "url",
        "format",
        "kernel",
        "target",
        "last_error",
        "revision",
        "last_sync",
        "managed",
    ] {
        let mut changed = sub.clone();
        match field {
            "url" => changed.url.push_str("changed"),
            "format" => changed.format = "clash".into(),
            "kernel" => changed.kernel = "other".into(),
            "target" => changed.target_proxy_config_id = Some("replacement".into()),
            "last_error" => changed.last_error = Some("result unknown".into()),
            "revision" => changed.updated_at_unix_ms += 1,
            "last_sync" => changed.last_sync_at_unix_ms = Some(11),
            _ => {
                changed.managed_source = Some(
                    serde_json::from_value(
                        json!({"pluginId":"p","providerId":"s","remoteSubscriptionId":"r"}),
                    )
                    .unwrap(),
                )
            }
        };
        assert!(!cache.matches(&changed, "custom", &profile), "{field}");
    }
    assert!(!cache.matches(&sub, "changed-UA", &profile));
    let mut changed = profile.clone();
    changed.content = Some(json!({"bad":"corrupt"}));
    assert!(!cache.matches(&sub, "custom", &changed));
    changed = profile.clone();
    changed.updated_at_unix_ms += 1;
    assert!(!cache.matches(&sub, "custom", &changed));
    let mut changed = sub.clone();
    changed.enabled = false;
    assert!(cache.matches(&changed, "custom", &profile));
}
#[test]
fn private_cache_survives_reopen_and_uses_revision_cas_without_source_secrets() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let (sub, profile) = fixture();
    let cache = Cache::confirmed(&sub, "custom", &profile, &validators()).unwrap();
    write(root.path(), &manager, &sub.id, &cache, None).unwrap();
    let (restored, revision) = read(root.path(), &Manager::default(), &sub.id).unwrap();
    assert!(restored.unwrap().matches(&sub, "custom", &profile));
    assert!(revision.is_some());
    assert!(write(root.path(), &manager, &sub.id, &cache, None).is_err());
    write(root.path(), &manager, &sub.id, &cache, revision).unwrap();
    let bytes = serde_json::to_vec(&cache).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("secret-canary"));
    assert!(!text.contains("custom-canary"));
    assert!(!text.contains("example.invalid"));
    let other = manager
        .admit(
            "plugin.other".into(),
            BTreeSet::from([Permission::new("storage.read", "self")]),
            BTreeSet::from([Permission::new("storage.read", "self")]),
            &BTreeSet::from([Permission::new("storage.read", "self")]),
        )
        .unwrap();
    let grants = BTreeSet::from([Permission::new("storage.read", "self")]);
    other.authorize(grants, Duration::from_secs(30)).unwrap();
    let other = other
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 65544,
                timeout: Duration::from_secs(10),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    assert!(storage::get(root.path(), &other, &key(&sub.id))
        .unwrap()
        .take(&other)
        .unwrap()
        .is_none());
}
#[test]
fn malformed_and_header_injection_cache_is_not_silently_trusted() {
    let (sub, profile) = fixture();
    let cache = Cache::confirmed(&sub, "custom", &profile, &validators()).unwrap();
    for bytes in [b"broken".to_vec(), b"{\"schema\":999}".to_vec()] {
        assert!(decode(&bytes).is_err());
    }
    let mut changed = cache.clone();
    changed.etag = Some("canary\r\nx-header: secret".into());
    assert!(decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    changed = cache.clone();
    changed.etag = Some("x".repeat(4097));
    assert!(decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let access = lease(&manager).unwrap();
    storage::change(
        root.path(),
        &access,
        &[Change::Put {
            key: key(&sub.id),
            expected: None,
            value: b"bad".to_vec(),
        }],
    )
    .unwrap()
    .take(&access)
    .unwrap();
    assert_eq!(
        read(root.path(), &manager, &sub.id).err().unwrap().code,
        "subscription_cache_error"
    );
}
#[test]
fn not_modified_only_advances_check_preserves_material_overrides_and_metadata() {
    let (sub, profile) = fixture();
    let before = serde_json::to_value(&sub).unwrap();
    let material = serde_json::to_value(&profile).unwrap();
    let updated = checked(&sub, 100, &SubscriptionUserinfo::default());
    let mut after = serde_json::to_value(&updated).unwrap();
    after["updatedAtUnixMs"] = before["updatedAtUnixMs"].clone();
    after["lastSyncAtUnixMs"] = before["lastSyncAtUnixMs"].clone();
    assert_eq!(after, before);
    assert_eq!(serde_json::to_value(&profile).unwrap(), material);
    let cache = Cache::confirmed(&updated, "custom", &profile, &validators()).unwrap();
    assert!(cache.matches(&updated, "custom", &profile));
    assert!(!cache.matches(&sub, "custom", &profile));
}
#[test]
fn actual_http_200_then_304_sends_persisted_validators_and_source_edit_sends_none() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/source?token=canary",
        listener.local_addr().unwrap()
    );
    let worker = std::thread::spawn(move || {
        for turn in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                request.push_str(&line)
            }
            let request = request.to_lowercase();
            if turn == 1 {
                assert!(request.contains("if-none-match: \"v1\""));
                assert!(request.contains("if-modified-since:"));
                stream
                    .write_all(b"HTTP/1.1 304 Not Modified\r\nsubscription-userinfo: download=7\r\nConnection: close\r\n\r\n")
                    .unwrap();
            } else {
                assert!(!request.contains("if-none-match:"));
                let body=general_purpose::STANDARD.encode(r#"{"inbounds":[],"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],"route":{"final":{"type":"direct"}}}"#);
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nLast-Modified: Mon, 12 Oct 2026 01:00:00 GMT\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        }
    });
    let state = AppState::default();
    let (mut sub, mut profile) = fixture();
    sub.url = url.clone();
    sub.enabled = false; // Automatic participation disabled: explicit sync remains usable.
    let first = fetch_subscription_conditional_blocking(
        state.capabilities(),
        &url,
        "custom",
        &Validators::default(),
    )
    .unwrap();
    assert!(!first.not_modified);
    // The same real parser is required before the confirmed material can become a base.
    let parsed = parse_subscription_content(&first.content, "zero").unwrap();
    profile.content = Some(parsed.content);
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::confirmed(&sub, "custom", &profile, &first.validators).unwrap();
    write(root.path(), state.capabilities(), &sub.id, &cache, None).unwrap();
    let (cache, _) = read(root.path(), state.capabilities(), &sub.id).unwrap();
    let cache = cache.unwrap();
    assert!(cache.matches(&sub, "custom", &profile));
    let second = fetch_subscription_conditional_blocking(
        state.capabilities(),
        &url,
        "custom",
        &cache.validators(),
    )
    .unwrap();
    assert!(second.not_modified);
    assert!(second.content.is_empty());
    let checked = checked(&sub, 100, &second.userinfo);
    assert_eq!(checked.upload_bytes, Some(5));
    assert_eq!(checked.download_bytes, Some(7));
    assert!(cache.matches_material(&profile));
    sub.url.push_str("&edited=1");
    assert!(!cache.matches(&sub, "custom", &profile));
    let third = fetch_subscription_conditional_blocking(
        state.capabilities(),
        &sub.url,
        "custom",
        &Validators::default(),
    )
    .unwrap();
    assert!(!third.not_modified);
    assert!(!format!("{:?}", state.capabilities().operations()).contains("canary"));
    worker.join().unwrap();
}
#[test]
fn actual_state_confirmation_persists_before_publish_and_rejects_late_edits() {
    let (sub, profile) = fixture();
    let state = AppState::default();
    state.subscriptions().lock().unwrap().push(sub.clone());
    state.proxy_configs().lock().unwrap().push(profile.clone());
    let cache = Cache::confirmed(&sub, "custom", &profile, &validators()).unwrap();
    let before = serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap();
    let guard = begin_in_flight(state.subscription_syncs(), "subscription", &sub.id).unwrap();
    assert!(confirm_not_modified(
        &state,
        &sub,
        "custom",
        &cache,
        &Default::default(),
        20,
        |_| Err(AppError::internal("injected storage failure"))
    )
    .is_err());
    assert_eq!(
        state.subscriptions().lock().unwrap()[0].last_sync_at_unix_ms,
        Some(10)
    );
    // An edit after the HTTP response must not be overwritten by the old snapshot.
    state.proxy_configs().lock().unwrap()[0].updated_at_unix_ms += 1;
    let mut called = false;
    assert!(confirm_not_modified(
        &state,
        &sub,
        "custom",
        &cache,
        &Default::default(),
        20,
        |_| {
            called = true;
            Ok(())
        }
    )
    .is_err());
    assert!(!called);
    state.proxy_configs().lock().unwrap()[0] = profile.clone();
    state.subscriptions().lock().unwrap()[0]
        .policy_selections
        .insert("group".into(), "new".into());
    assert!(confirm_not_modified(
        &state,
        &sub,
        "custom",
        &cache,
        &Default::default(),
        20,
        |_| panic!("stale callback must not persist")
    )
    .is_err());
    state.subscriptions().lock().unwrap()[0] = sub.clone();
    let (updated, refreshed) = confirm_not_modified(
        &state,
        &sub,
        "custom",
        &cache,
        &Default::default(),
        20,
        |values| {
            assert_eq!(values[0].last_sync_at_unix_ms, Some(20));
            Ok(())
        },
    )
    .unwrap();
    assert!(refreshed.matches(&updated, "custom", &profile));
    assert_eq!(
        serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap(),
        before
    );
    assert_eq!(updated.policy_selections, sub.policy_selections);
    // Only private storage/network capabilities were used: no configuration apply.
    assert!(state.capabilities().operations().is_empty());
    drop(guard);
}
#[test]
fn unknown_runtime_errors_and_expiry_overflow_do_not_leak_or_fake_fresh_metadata() {
    let error = sanitized_apply_error(AppError {
        code: "unconfirmed",
        message: "https://example/sub?token=canary proxy-password=canary".into(),
        details: Some(json!({"body":"canary"})),
    });
    assert_eq!(error.code, "unconfirmed");
    assert!(error.details.is_none());
    assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
    let info = parse_subscription_userinfo("expire=18446744073709551615");
    assert!(info.expire_ms().is_none());
}
#[test]
fn transport_rejection_or_corrupt_utf8_preserves_last_valid_material_and_cache() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/sub?token=canary", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for reply in [b"HTTP/1.1 503 Unavailable\r\nContent-Length: 6\r\nConnection: close\r\n\r\ncanary".as_slice(),b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nETag: \"corrupt\"\r\nConnection: close\r\n\r\n\xff".as_slice()] {
            let(mut stream,_)=listener.accept().unwrap();let mut reader=BufReader::new(stream.try_clone().unwrap());loop{let mut line=String::new();reader.read_line(&mut line).unwrap();if line=="\r\n"{break}}stream.write_all(reply).unwrap();
        }
    });
    let state = AppState::default();
    let (sub, profile) = fixture();
    let cache = Cache::confirmed(&sub, "custom", &profile, &validators()).unwrap();
    let root = tempfile::tempdir().unwrap();
    write(root.path(), state.capabilities(), &sub.id, &cache, None).unwrap();
    let before = serde_json::to_vec(&cache).unwrap();
    let material = serde_json::to_value(&profile).unwrap();
    for _ in 0..2 {
        let error = fetch_subscription_conditional_blocking(
            state.capabilities(),
            &url,
            "custom",
            &cache.validators(),
        )
        .unwrap_err();
        assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
        let (restored, _) = read(root.path(), state.capabilities(), &sub.id).unwrap();
        assert_eq!(serde_json::to_vec(&restored.unwrap()).unwrap(), before);
        assert_eq!(serde_json::to_value(&profile).unwrap(), material);
    }
    worker.join().unwrap();
}
#[test]
fn only_exact_host_uncertainty_phase_blocks_repeating_network_and_apply() {
    let (mut source, _) = fixture();
    for reason in [
        "previous network timeout",
        "provider said unknown",
        "legacy arbitrary error string",
        "invalid source",
    ] {
        source.last_error = Some(reason.into());
        assert!(require_confirmed_previous_result(&source).is_ok());
    }
    let normalized = sanitized_apply_error(AppError {
        code: "config_apply_uncertain",
        message: "canary raw core body".into(),
        details: Some(json!({"secret":"canary"})),
    });
    assert_eq!(normalized.message, UNKNOWN_APPLY_MESSAGE);
    source.last_error = Some(normalized.message);
    assert_eq!(
        require_confirmed_previous_result(&source).unwrap_err().code,
        "config_apply_uncertain"
    );
    source.enabled = false;
    assert!(require_confirmed_previous_result(&source).is_err());
}

#[test]
fn explicit_reset_repairs_malformed_private_row_only_and_retains_unknown_barrier() {
    let root = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let (mut source, profile) = fixture();
    source.last_error = Some(UNKNOWN_APPLY_MESSAGE.into());
    state.subscriptions().lock().unwrap().push(source.clone());
    state.proxy_configs().lock().unwrap().push(profile.clone());
    let access = lease(state.capabilities()).unwrap();
    storage::change(
        root.path(),
        &access,
        &[Change::Put {
            key: key(&source.id),
            expected: None,
            value: b"malformed-cache-canary".to_vec(),
        }],
    )
    .unwrap()
    .take(&access)
    .unwrap();
    let other = "unrelated-source";
    let access = lease(state.capabilities()).unwrap();
    storage::change(
        root.path(),
        &access,
        &[Change::Put {
            key: key(other),
            expected: None,
            value: b"other-row".to_vec(),
        }],
    )
    .unwrap()
    .take(&access)
    .unwrap();
    assert!(read(root.path(), state.capabilities(), &source.id).is_err());
    let before = serde_json::to_value(state.subscriptions().lock().unwrap().clone()).unwrap();
    let materials = serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap();
    let reset = reset_at(&state, root.path(), &source.id, source.updated_at_unix_ms).unwrap();
    assert!(reset.cleared);
    assert_eq!(
        read(root.path(), state.capabilities(), &source.id)
            .unwrap()
            .1,
        None
    );
    let access = lease(state.capabilities()).unwrap();
    assert_eq!(
        storage::get(root.path(), &access, &key(other))
            .unwrap()
            .take(&access)
            .unwrap()
            .unwrap()
            .value,
        b"other-row"
    );
    assert!(
        !reset_at(&state, root.path(), &source.id, source.updated_at_unix_ms)
            .unwrap()
            .cleared
    );
    assert_eq!(
        serde_json::to_value(state.subscriptions().lock().unwrap().clone()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap(),
        materials
    );
    assert_eq!(
        require_confirmed_previous_result(&state.subscriptions().lock().unwrap()[0])
            .unwrap_err()
            .code,
        "config_apply_uncertain"
    );
    for operation in state.capabilities().operations() {
        assert!(operation.capability.starts_with("storage."));
        assert!(!format!("{operation:?}").contains("malformed-cache-canary"));
    }
}

#[test]
fn reset_requires_source_revision_and_shared_claim_and_denies_managed() {
    let root = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let (source, profile) = fixture();
    state.subscriptions().lock().unwrap().push(source.clone());
    let cache = Cache::confirmed(&source, "custom", &profile, &validators()).unwrap();
    write(root.path(), state.capabilities(), &source.id, &cache, None).unwrap();
    assert_eq!(
        reset_at(
            &state,
            root.path(),
            &source.id,
            source.updated_at_unix_ms + 1
        )
        .err()
        .unwrap()
        .code,
        "conflict"
    );
    assert!(read(root.path(), state.capabilities(), &source.id)
        .unwrap()
        .0
        .is_some());
    let active_sync =
        begin_in_flight(state.subscription_syncs(), "subscription", &source.id).unwrap();
    assert_eq!(
        reset_at(&state, root.path(), &source.id, source.updated_at_unix_ms)
            .err()
            .unwrap()
            .code,
        "conflict"
    );
    drop(active_sync);
    state.subscriptions().lock().unwrap()[0].managed_source = Some(
        serde_json::from_value(json!({"pluginId":"p","providerId":"s","remoteSubscriptionId":"r"}))
            .unwrap(),
    );
    assert_eq!(
        reset_at(&state, root.path(), &source.id, source.updated_at_unix_ms)
            .err()
            .unwrap()
            .code,
        "invalid_argument"
    );
    assert!(read(root.path(), state.capabilities(), &source.id)
        .unwrap()
        .0
        .is_some());
}

#[test]
fn reset_cas_cannot_remove_row_replaced_after_observation() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let (source, profile) = fixture();
    let cache = Cache::confirmed(&source, "custom", &profile, &validators()).unwrap();
    write(root.path(), &manager, &source.id, &cache, None).unwrap();
    let (_, revision) = read(root.path(), &manager, &source.id).unwrap();
    write(root.path(), &manager, &source.id, &cache, revision).unwrap();
    assert_eq!(
        delete_observed(root.path(), &manager, &source.id, revision.unwrap())
            .unwrap_err()
            .code,
        "conflict"
    );
    assert!(read(root.path(), &manager, &source.id).unwrap().0.is_some());
}
