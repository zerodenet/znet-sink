use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use znet_plugin_sandbox::{
    contract::{sha256, Limits, Manifest},
    distribution::{
        directory::Registration,
        package::{self, Payload, SourceComponent},
    },
};
const SEED: [u8; 32] = [17; 32];
fn registration() -> Registration {
    serde_json::from_value(serde_json::json!({"id":"org.example.plugin","repository":"https://github.com/example/plugin","publisher":{"id":"example","public_key":STANDARD.encode(ed25519_dalek::SigningKey::from_bytes(&SEED).verifying_key().to_bytes())},"name":"Example","description":"Test","license":"MIT","maintainers":["example"],"release_source":{"type":"github-releases","metadata_asset":"marketplace-entry.json"},"surfaces":[],"capabilities":["plugin.self.read","network.get","network.request","records.summary.read"]})).unwrap()
}
fn setup(unsupported: bool) -> (tempfile::TempDir, Host, Manager) {
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let request = if unsupported {
        serde_json::json!({"capability":"records.summary.read","scope":"selection:test"})
    } else {
        serde_json::json!({"capability":"plugin.self.read","scope":"self"})
    };
    setup_component(source, request)
}
fn setup_component(source: &str, request: serde_json::Value) -> (tempfile::TempDir, Host, Manager) {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let manifest: Manifest = serde_json::from_value(serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":"1.0.0","requires_host":"=0.0.1","api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[request],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()})).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    let directory = Directory {
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: vec![registration()],
    };
    host.store()
        .unwrap()
        .install(
            &bytes,
            &directory.plugins[0],
            &Target::native_desktop().unwrap(),
            "0.0.1",
        )
        .unwrap();
    host.state.lock().unwrap().directory = Some((directory, Instant::now()));
    host.rescan(&manager, &host.store().unwrap()).unwrap();
    (root, host, manager)
}
fn approve(host: &Host, manager: &Manager) -> Review {
    let snapshot = host.snapshot(manager);
    let row = &snapshot.components[0];
    let review = row.review.clone().unwrap();
    assert!(
        host.run(manager, review.clone()).is_err(),
        "installation is not authorization"
    );
    let grants = row.permissions.iter().map(|p| p.request.clone()).collect();
    host.authorize(manager, review, grants).unwrap().components[0]
        .review
        .clone()
        .unwrap()
}
#[test]
fn signed_install_review_authorize_real_vm_and_stop_share_the_client_manager() {
    let (_root, host, manager) = setup(false);
    let review = approve(&host, &manager);
    let result = host.run(&manager, review.clone()).unwrap();
    assert_eq!(result["plugin_id"], "org.example.plugin");
    assert!(manager
        .operations()
        .iter()
        .any(|o| o.capability == "plugin.self.read"));
    host.stop(&manager, &review.key);
    assert!(host.run(&manager, review).is_err());
}
#[test]
fn unsupported_required_permissions_and_forged_grants_are_denied() {
    let (_root, host, manager) = setup(true);
    let snapshot = host.snapshot(&manager);
    assert!(snapshot.components[0].blocked.is_some());
    assert!(!snapshot.components[0].permissions[0].supported);
    let (_root, host, manager) = setup(false);
    let review = host.snapshot(&manager).components[0]
        .review
        .clone()
        .unwrap();
    assert!(host
        .authorize(
            &manager,
            review,
            vec![Request {
                capability: Capability::NetworkGet,
                scope: "https://example.org".into()
            }]
        )
        .is_err());
}
#[test]
fn tampering_or_registry_withdrawal_revokes_an_already_authorized_component() {
    for tamper in [false, true] {
        let (root, host, manager) = setup(false);
        let review = approve(&host, &manager);
        if tamper {
            std::fs::write(root.path().join("installed.json"), b"invalid").unwrap();
        } else {
            host.state
                .lock()
                .unwrap()
                .directory
                .as_mut()
                .unwrap()
                .0
                .plugins
                .clear();
        }
        assert!(host.run(&manager, review.clone()).is_err());
        assert!(manager.component_snapshots().iter().all(|p| !p.enabled));
    }
}
#[test]
fn uninstall_and_expired_registry_prevent_execution() {
    let (_root, host, manager) = setup(false);
    let review = approve(&host, &manager);
    host.uninstall(&manager, "org.example.plugin").unwrap();
    assert!(host.run(&manager, review).is_err());
    let (_root, host, manager) = setup(false);
    let review = approve(&host, &manager);
    host.state.lock().unwrap().directory.as_mut().unwrap().1 = Instant::now() - DIRECTORY_TTL;
    assert!(host.run(&manager, review).is_err());
    assert!(!host.snapshot(&manager).checked);
}

#[test]
fn signed_network_component_requires_approval_and_stop_prevents_more_requests() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let call = serde_json::json!({"capability":"network.request","scope":origin,"url":origin,"method":"POST","body":"sample"});
    let source = format!(
        "JSON.parse(hostCall({})).status",
        serde_json::to_string(&call.to_string()).unwrap()
    );
    let (_root, host, manager) = setup_component(
        &source,
        serde_json::json!({"capability":"network.request","scope":origin}),
    );
    assert!(host.snapshot(&manager).components[0].permissions[0].supported);
    let review = approve(&host, &manager);
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    let server = std::thread::spawn(move || {
        let until = Instant::now() + std::time::Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(e) => panic!("request did not arrive: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"sample") {
            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buf[..n]);
        }
        assert!(request.starts_with(b"POST / HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        listener
    });
    assert_eq!(
        host.run(&manager, review.clone()).unwrap(),
        serde_json::json!(200)
    );
    let listener = server.join().unwrap();
    host.stop(&manager, &review.key);
    assert!(host.run(&manager, review).is_err());
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
}
