use super::*;
use std::net::UdpSocket;

fn exchange(change: impl FnOnce(&mut Vec<u8>) + Send + 'static) -> AppResult<()> {
    // Keep the actual reservation until the responder owns it.
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let addr = socket.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut request = [0; 512];
        let (size, peer) = socket.recv_from(&mut request).unwrap();
        let mut reply = request[..size].to_vec();
        reply[2] = 0x81;
        reply[3] = 0x83; // NXDOMAIN is a healthy answer for .invalid.
        change(&mut reply);
        socket.send_to(&reply, peer).unwrap();
    });
    let result = probe_at(addr);
    worker.join().unwrap();
    result
}
#[test]
fn dns_probe_accepts_negative_answers_but_rejects_failures_and_unrelated_packets() {
    exchange(|_| {}).unwrap();
    assert!(exchange(|reply| reply[0] ^= 1).is_err());
    assert!(exchange(|reply| reply[3] = 0x82).is_err()); // SERVFAIL
    assert!(exchange(|reply| reply[2] |= 2).is_err()); // truncated
    assert!(exchange(|reply| reply.truncate(8)).is_err());
}
