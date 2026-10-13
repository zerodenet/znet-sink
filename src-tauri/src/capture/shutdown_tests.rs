use super::stop_owned_tun;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
};

#[test]
fn exit_persists_tun_off_for_next_launch_without_losing_network_settings() {
    use crate::models::app_config::AppConfig;
    use crate::services::app_config_store;

    for intent in [Some(true), None, Some(false)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app-config.json");
        let mut config = AppConfig::default();
        config.tun.enabled = intent;
        config.tun.addr = "10.77.0.1/24".into();
        config.tun.mask = "255.255.255.0".into();
        config.tun.mtu = 1280;
        config.core.auto_connect = false;
        config.core.cleanup_proxy_on_exit = false;
        config.dns.enabled = false;
        config
            .client_edits
            .as_mut()
            .unwrap()
            .insert("tun.mtu".into(), json!(1400));
        // Live AppState has already passed the store's load normalization.
        crate::services::bypass::normalize(&mut config).unwrap();
        app_config_store::save(&path, &config).unwrap();
        let mut expected = config.clone();
        expected.tun.enabled = Some(false);

        super::clear_saved_tun_intent(&mut config, &path).unwrap();
        assert_eq!(config, expected);
        let loaded = app_config_store::load_or_default(&path).unwrap();
        assert_eq!(loaded, expected);
        // Even an imported TUN block cannot activate capture on startup;
        // composition strips it and restoration sees the saved OFF intent.
        let source = json!({"runtime":{"tun":{"enabled":true,"addr":"10.99.0.1/24"}}});
        let original = source.clone();
        let (app, resolved) =
            crate::configuration::local_edits::resolve(&loaded, Some("imported"), &source).unwrap();
        let candidate = crate::configuration::composition::finalize(
            &source,
            resolved,
            &crate::configuration::composition::Inputs {
                source_profile_id: Some("imported".into()),
                app,
                supports_tolerance: false,
                selections: Default::default(),
            },
        )
        .unwrap();
        assert!(candidate.config.pointer("/runtime/tun").is_none());
        assert_eq!(source, original);
        let bytes = std::fs::read(&path).unwrap();
        super::clear_saved_tun_intent(&mut config, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn exit_does_not_claim_saved_tun_off_when_storage_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = crate::models::app_config::AppConfig::default();
    config.tun.enabled = Some(true);
    let previous = config.clone();
    // A directory cannot be used as the configuration file.
    assert!(super::clear_saved_tun_intent(&mut config, dir.path()).is_err());
    assert_eq!(config, previous);
}

#[tokio::test]
async fn shutdown_checks_owned_peer_does_not_replay_and_confirms_stopped_state() {
    // Wrong peer, lost reply, unconfirmed stop, successful cleanup.
    for (peer_pid, lose_reply, still_enabled) in [
        (9, true, true),
        (7, true, true),
        (7, false, true),
        (7, false, false),
    ] {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = dir.path().join("cleanup.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let methods = Arc::new(Mutex::new(Vec::new()));
        let recorded = methods.clone();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut submitted = false;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = match request["type"].as_str().unwrap() {
                    "subscribe" => json!("subscribed"),
                    "query" if request["request"].get("runtime").is_some() => {
                        json!({"runtime":{"pid":peer_pid}})
                    }
                    "query" => json!({"tun_status":{"running":!submitted || still_enabled}}),
                    "command" => {
                        assert_eq!(request["method"], "tun.stop");
                        recorded.lock().unwrap().push("tun.stop");
                        if lose_reply {
                            break;
                        }
                        submitted = true;
                        json!({"accepted":true})
                    }
                    other => panic!("unexpected {other}"),
                };
                if writeln!(
                    writer,
                    "{}",
                    json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":result})
                )
                .is_err()
                {
                    break;
                }
            }
        });
        let result = stop_owned_tun(7, path.to_string_lossy().into_owned()).await;
        assert_eq!(
            result.is_ok(),
            peer_pid == 7 && !lose_reply && !still_enabled,
            "peer={peer_pid}, lost={lose_reply}, enabled={still_enabled}: {result:?}"
        );
        crate::kernel::connection::reset_endpoint(&crate::models::core::CoreEndpoint {
            transport: "unix".into(),
            path: path.to_string_lossy().into_owned(),
        });
        server.join().unwrap();
        assert_eq!(methods.lock().unwrap().len(), usize::from(peer_pid == 7));
    }
}

#[test]
fn stopped_with_cleanup_failure_is_not_reported_as_success() {
    let status = crate::models::zero_runtime::GuiTunStatus {
        enabled: false,
        last_error: Some("route removal failed".into()),
        ..Default::default()
    };
    assert!(super::stopped_without_error(status)
        .unwrap_err()
        .message
        .contains("route removal failed"));
}
