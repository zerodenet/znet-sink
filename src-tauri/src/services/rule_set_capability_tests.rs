use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};
#[tokio::test]
async fn managed_rule_download_retains_conditional_get_and_not_modified_state() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let source = RuleSetSource {
        url: format!("http://{}/rule", listener.local_addr().unwrap()),
        format: "domain".into(),
        user_agent: Some("test-rules".into()),
        update_interval_secs: Some(3600),
    };
    let worker = std::thread::spawn(move || {
        for cached in [false, true] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            if cached {
                assert!(request
                    .to_lowercase()
                    .contains("if-none-match: \"version-1\""));
                socket
                    .write_all(b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n")
                    .unwrap();
            } else {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nETag: \"version-1\"\r\nConnection: close\r\n\r\nexample.org").unwrap();
            }
        }
    });
    let manager = znet_client_core::capability::Manager::default();
    let FetchOutcome::Modified(first) = fetch_source(manager.clone(), &source, None).await.unwrap()
    else {
        panic!("expected body")
    };
    assert_eq!(first.bytes, b"example.org");
    let FetchOutcome::NotModified(next) =
        fetch_source(manager.clone(), &source, Some(&first.state))
            .await
            .unwrap()
    else {
        panic!("expected 304")
    };
    assert_eq!(next.content_sha256, first.state.content_sha256);
    assert_eq!(next.etag, first.state.etag);
    assert_eq!(manager.operations().len(), 2);
    worker.join().unwrap();
}
