use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};
#[test]
fn subscription_entry_uses_client_management_and_preserves_provider_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/subscription?token=canary",
        listener.local_addr().unwrap()
    );
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nsubscription-userinfo: upload=4; download=5; total=99; expire=100\r\nConnection: close\r\n\r\n{}").unwrap();
    });
    let state = AppState::default();
    let result =
        fetch_subscription_content_blocking(state.capabilities(), &url, "Clash.Meta").unwrap();
    assert_eq!(result.content, "{}");
    assert_eq!(result.userinfo.upload, Some(4));
    assert_eq!(result.userinfo.download, Some(5));
    assert_eq!(result.userinfo.expire_ms(), Some(100_000));
    let records = state.capabilities().operations();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].capability, "network.get");
    assert_eq!(
        records[0].state,
        znet_client_core::capability::OperationState::Completed
    );
    assert!(!format!("{records:?}").contains("canary"));
    worker.join().unwrap();
}
