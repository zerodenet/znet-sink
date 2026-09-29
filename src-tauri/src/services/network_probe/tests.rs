use super::probe_local_network;
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use znet_client_core::capability::Manager;

fn unavailable_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!(
        "http://{}/private?token=secret",
        listener.local_addr().unwrap()
    )
}

#[test]
fn all_failed_probes_identify_the_check_without_disclosing_configured_urls() {
    let error = probe_local_network(&Manager::default(), &[unavailable_url(), unavailable_url()])
        .unwrap_err();
    assert!(error.message.contains("公网 IP 与位置检查失败"));
    assert!(error.message.contains("不代表网络完全不可用"));
    assert!(error.message.contains("探测 1") && error.message.contains("探测 2"));
    assert!(error.message.contains("建立连接失败"));
    assert!(!error.message.contains("token") && !error.message.contains("private"));
}

#[test]
fn failed_first_probe_falls_back_to_a_valid_public_ip_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let _ = stream.read(&mut [0; 2048]);
        let body = r#"{"ip":"203.0.113.7","country":"JP"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let result = probe_local_network(&Manager::default(), &[unavailable_url(), url]).unwrap();
    assert_eq!(result.ip, "203.0.113.7");
    assert_eq!(result.country.as_deref(), Some("JP"));
    worker.join().unwrap();
}
