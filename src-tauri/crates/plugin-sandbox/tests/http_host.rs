use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_plugin_sandbox::{
    contract::*,
    policy::Authority,
    runtime::{execute, execute_for_host},
};
#[test]
fn production_host_request_uses_authorized_origin_and_common_execution() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let source = format!("JSON.parse(hostCall(JSON.stringify({{capability:'network.request',scope:{0:?},url:{0:?},method:'POST',headers:{{'content-type':'application/json'}},body:'{{\"login\":\"sample\"}}'}}))).status",origin);
    let manifest = serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.network","component_id":"request","version":"1.0.0","requires_host":"=0.0.1","api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[{"capability":"network.request","scope":origin}],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()});
    let component = Component::load(&serde_json::to_vec(&manifest).unwrap(), &source).unwrap();
    let manager = znet_client_core::capability::Manager::default();
    let grants: BTreeSet<_> = component.manifest().required.iter().cloned().collect();
    let authority = Authority::admit_with_manager(&component, &grants, &manager).unwrap();
    authority
        .authorize(grants, Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        execute(
            &component,
            &authority,
            None,
            Arc::new(AtomicBool::new(false))
        ),
        Err(Error::PermissionDenied)
    );
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut size = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.to_lowercase().starts_with("content-length:") {
                size = line
                    .split(':')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
            }
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; size];
        reader.read_exact(&mut body).unwrap();
        assert_eq!(body, b"{\"login\":\"sample\"}");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
    });
    assert_eq!(
        execute_for_host(
            &component,
            &authority,
            None,
            Arc::new(AtomicBool::new(false)),
            "0.0.1"
        )
        .unwrap(),
        serde_json::json!(200)
    );
    worker.join().unwrap();
    assert_eq!(
        manager.operations().last().unwrap().capability,
        "network.request"
    );
}
