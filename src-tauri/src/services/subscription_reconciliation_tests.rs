use super::*;
use crate::models::proxy_config::ProxyConfigProfile;
use serde_json::json;
fn state_fixture() -> AppState {
    let state = AppState::default();
    let source=serde_json::from_value(json!({"id":"ordinary","name":"Source","url":"https://example.invalid/?token=secret-canary","enabled":false,"kernel":"zero","format":"auto","targetProxyConfigId":"material","updatedAtUnixMs":15,"lastSyncAtUnixMs":10,"lastError":UNKNOWN_APPLY_MESSAGE,"policySelections":{"group":"direct"}})).unwrap();
    state.subscriptions().lock().unwrap().push(source);
    state.proxy_configs().lock().unwrap().push(ProxyConfigProfile { policy_selections: Default::default(),id:"material".into(),name:"Saved".into(),kernel:"zero".into(),format:"zero-json".into(),path:None,content:Some(json!({"inbounds":[],"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],"route":{"final":{"type":"direct"}}})),active:false,managed_source:None,updated_at_unix_ms:11,capabilities:Default::default()});
    state
}
#[test]
fn local_unknown_facts_are_read_only_even_when_runtime_is_readable() {
    let state = state_fixture();
    let source = serde_json::to_value(state.subscriptions().lock().unwrap().clone()).unwrap();
    let profiles = serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap();
    let before = local_snapshot(&state, "ordinary", 15).unwrap();
    assert_eq!(before.material.status, "saved-valid");
    assert_eq!(before.material.validation, "native-schema-only");
    let result = finish(
        &state,
        "ordinary",
        15,
        before,
        RuntimeFacts {
            status: "readable",
            core_instance_id: Some("one".into()),
            config_revision: Some("9007199254740993".into()),
            reason_code: "runtime_identity_only",
            reason: "readable",
        },
    )
    .unwrap();
    assert_eq!(result.status, "result-unknown");
    assert!(!result.can_resolve);
    assert_eq!(result.evidence.candidate, "not-recorded");
    assert_eq!(result.evidence.receipt, "not-recorded");
    assert_eq!(
        result.runtime.config_revision.as_deref(),
        Some("9007199254740993")
    );
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("secret-canary"));
    assert_eq!(
        serde_json::to_value(state.subscriptions().lock().unwrap().clone()).unwrap(),
        source
    );
    assert_eq!(
        serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap(),
        profiles
    );
    assert!(state.capabilities().operations().is_empty());
}
#[test]
fn missing_invalid_and_managed_material_are_separate_and_source_guards_are_strict() {
    let state = state_fixture();
    assert_eq!(
        local_snapshot(&state, "ordinary", 16).err().unwrap().code,
        "conflict"
    );
    state.proxy_configs().lock().unwrap()[0].content = Some(json!({"secret":"corrupt-canary"}));
    assert_eq!(
        local_snapshot(&state, "ordinary", 15)
            .unwrap()
            .material
            .status,
        "saved-invalid"
    );
    state.proxy_configs().lock().unwrap()[0].managed_source = Some(
        serde_json::from_value(json!({"pluginId":"p","providerId":"s","remoteSubscriptionId":"r"}))
            .unwrap(),
    );
    assert_eq!(
        local_snapshot(&state, "ordinary", 15)
            .unwrap()
            .material
            .status,
        "ownership-mismatch"
    );
    state.proxy_configs().lock().unwrap().clear();
    assert_eq!(
        local_snapshot(&state, "ordinary", 15)
            .unwrap()
            .material
            .status,
        "missing"
    );
    state.subscriptions().lock().unwrap()[0].target_proxy_config_id = None;
    assert_eq!(
        local_snapshot(&state, "ordinary", 15)
            .unwrap()
            .material
            .status,
        "not-linked"
    );
    state.subscriptions().lock().unwrap()[0].last_error = Some("legacy failure".into());
    assert_eq!(
        local_snapshot(&state, "ordinary", 15).err().unwrap().code,
        "invalid_argument"
    );
    state.subscriptions().lock().unwrap()[0].managed_source = Some(
        serde_json::from_value(json!({"pluginId":"p","providerId":"s","remoteSubscriptionId":"r"}))
            .unwrap(),
    );
    assert_eq!(
        local_snapshot(&state, "ordinary", 15).err().unwrap().code,
        "invalid_argument"
    );
}
#[test]
fn late_same_revision_source_and_material_changes_cannot_publish_old_inspection() {
    for change_source in [true, false] {
        let state = state_fixture();
        let before = local_snapshot(&state, "ordinary", 15).unwrap();
        if change_source {
            state.subscriptions().lock().unwrap()[0]
                .url
                .push_str("changed");
        } else {
            state.proxy_configs().lock().unwrap()[0].content =
                Some(json!({"different":"secret-canary"}));
        }
        assert_eq!(
            finish(&state, "ordinary", 15, before, RuntimeFacts::unavailable())
                .unwrap_err()
                .code,
            "conflict"
        );
    }
}
#[tokio::test]
async fn mobile_read_reports_unsupported_without_entering_ipc_or_clearing_local_error() {
    let state = state_fixture();
    let before = local_snapshot(&state, "ordinary", 15).unwrap();
    let runtime = runtime_identity(
        state.capabilities(),
        CoreIpcOptions {
            socket: Some("must-not-open-canary".into()),
            timeout_ms: Some(1),
        },
        true,
    )
    .await;
    let result = finish(&state, "ordinary", 15, before, runtime).unwrap();
    assert_eq!(result.runtime.status, "unsupported");
    assert!(result.runtime.core_instance_id.is_none());
    assert!(!result.can_resolve);
    assert_eq!(
        state.subscriptions().lock().unwrap()[0]
            .last_error
            .as_deref(),
        Some(UNKNOWN_APPLY_MESSAGE)
    );
    assert!(state.capabilities().operations().is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn actual_pinned_runtime_identity_queries_are_typed_bounded_and_never_apply() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        sync::{Arc, Mutex},
        thread,
    };
    for case in [
        "stable",
        "instance-changed",
        "revision-changed",
        "missing-revision",
        "denied",
        "disconnect",
    ] {
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let socket = root.path().join("inspection.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reads = 0;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let frame: serde_json::Value = serde_json::from_str(&line).unwrap();
                let reply = if frame["type"] == "subscribe" {
                    json!({"api_id":"zero.api.v1","id":frame["id"],"ok":true,"result":"subscribed"})
                } else {
                    assert_eq!(frame["type"], "query");
                    assert_eq!(frame["request"], json!({"runtime":{}}));
                    recorded.lock().unwrap().push(frame["request"].clone());
                    reads += 1;
                    if case == "disconnect" {
                        break;
                    }
                    if case == "denied" {
                        json!({"api_id":"zero.api.v1","id":frame["id"],"ok":false,"error":{"code":"permission_denied","message":"server-error-canary"}})
                    } else {
                        let revision = if case == "revision-changed" && reads == 2 {
                            9007199254740994u64
                        } else {
                            9007199254740993u64
                        };
                        let instance = if case == "instance-changed" && reads == 2 {
                            "new"
                        } else {
                            "one"
                        };
                        let mut runtime = json!({"core_instance_id":instance,"config_revision":revision,"config_path":"secret-path-canary","active_sessions":[{"secret":"payload-canary"}]});
                        if case == "missing-revision" {
                            runtime.as_object_mut().unwrap().remove("config_revision");
                        }
                        json!({"api_id":"zero.api.v1","id":frame["id"],"ok":true,"result":{"runtime":runtime}})
                    }
                };
                writeln!(writer, "{reply}").unwrap();
                if reads == 2 || reads == 1 && matches!(case, "denied" | "missing-revision") {
                    break;
                }
            }
        });
        let state = state_fixture();
        let options = CoreIpcOptions {
            socket: Some(socket.to_string_lossy().into_owned()),
            timeout_ms: Some(1000),
        };
        let result = runtime_identity(state.capabilities(), options.clone(), false).await;
        match case {
            "stable" => {
                assert_eq!(result.status, "readable");
                assert_eq!(result.config_revision.as_deref(), Some("9007199254740993"));
                assert_eq!(
                    observed_identity(&result),
                    Some(KernelRuntimeIdentity {
                        core_instance_id: "one".into(),
                        config_revision: 9007199254740993,
                    })
                );
            }
            "instance-changed" | "revision-changed" => {
                assert_eq!(result.status, "changed");
                assert!(result.config_revision.is_none());
                assert!(observed_identity(&result).is_none());
            }
            _ => {
                assert_eq!(result.status, "unavailable");
                assert!(result.core_instance_id.is_none());
                assert!(observed_identity(&result).is_none());
            }
        }
        let text = serde_json::to_string(&result).unwrap();
        assert!(!text.contains("canary"));
        assert_eq!(
            state.subscriptions().lock().unwrap()[0]
                .last_error
                .as_deref(),
            Some(UNKNOWN_APPLY_MESSAGE)
        );
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
        );
        worker.join().unwrap();
        assert!(calls.lock().unwrap().len() <= 2);
        for operation in state.capabilities().operations() {
            assert_eq!(operation.capability, "runtime.inspect");
            assert!(!format!("{operation:?}").contains("canary"));
        }
    }
}
