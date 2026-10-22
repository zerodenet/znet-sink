use super::launch::PRIVILEGED_COMMAND_SCRIPT;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use super::{
    dns_helper, execute_with_helper, request, validate_networksetup_command, HelperRequest,
    HelperResponse,
};

#[test]
fn dns_installation_and_refresh_cannot_obtain_fresh_authority() {
    let mut slot = None;
    // First install, target replacement after a health flap, and repeated
    // polls all use this same noninteractive channel acquisition boundary.
    for _ in 0..3 {
        let error = dns_helper(&mut slot).unwrap_err();
        assert!(error.message.contains("后台维护不会请求新授权"));
        assert!(slot.is_none());
    }
}

#[test]
fn background_dns_refresh_reuses_authority_obtained_for_proxy_operations() {
    let (parent, worker) = helper_channel(1, true);
    let mut slot = Some(parent);
    let stream = dns_helper(&mut slot).unwrap();
    assert!(request(stream, &commands()).unwrap().success);
    worker.join().unwrap();
}

#[test]
fn explicit_authorization_health_check_does_not_modify_network_preferences() {
    let response = super::run_networksetup_as_root(&[]);
    assert!(response.success);
    assert!(response.stdout.is_empty());
    assert!(response.stderr.is_empty());
}

fn commands() -> Vec<Vec<String>> {
    vec![vec![
        "-setwebproxystate".into(),
        "Wi-Fi".into(),
        "off".into(),
    ]]
}

fn helper_channel(count: usize, success: bool) -> (UnixStream, thread::JoinHandle<()>) {
    let (parent, mut helper) = UnixStream::pair().unwrap();
    parent
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let worker = thread::spawn(move || {
        for _ in 0..count {
            let mut line = String::new();
            assert!(BufReader::new(&mut helper).read_line(&mut line).unwrap() > 0);
            let request: HelperRequest = serde_json::from_str(&line).unwrap();
            assert!(matches!(request, HelperRequest::NetworkSetup { .. }));
            serde_json::to_writer(
                &mut helper,
                &HelperResponse {
                    success,
                    code: if success { 0 } else { 1 },
                    stdout: Vec::new(),
                    stderr: if success {
                        Vec::new()
                    } else {
                        b"cleanup denied".to_vec()
                    },
                },
            )
            .unwrap();
            helper.write_all(b"\n").unwrap();
            helper.flush().unwrap();
        }
    });
    (parent, worker)
}

#[test]
fn exit_reuses_existing_authorization_without_launching_an_authorizer() {
    let (parent, worker) = helper_channel(1, true);
    let mut slot = Some(parent);
    execute_with_helper(
        &mut slot,
        || false,
        || panic!("quit must not authorize"),
        &commands(),
    )
    .unwrap();
    worker.join().unwrap();
}

#[test]
fn exit_without_existing_authorization_does_not_launch_an_authorizer() {
    let mut slot = None;
    let error = execute_with_helper(
        &mut slot,
        || false,
        || panic!("quit must not authorize"),
        &commands(),
    )
    .unwrap_err();
    assert!(error
        .message
        .contains("no prompt is allowed during shutdown"));
    assert!(slot.is_none());
}

#[test]
fn exit_with_a_lost_helper_does_not_reauthorize() {
    let (parent, helper) = UnixStream::pair().unwrap();
    drop(helper);
    let mut slot = Some(parent);
    let error = execute_with_helper(
        &mut slot,
        || false,
        || panic!("quit must not reauthorize"),
        &commands(),
    )
    .unwrap_err();
    assert!(error
        .message
        .contains("no prompt is allowed during shutdown"));
    assert!(slot.is_none());
}

#[test]
fn rejected_exit_cleanup_is_reported_without_reauthorization() {
    let (parent, worker) = helper_channel(1, false);
    let mut slot = Some(parent);
    let error = execute_with_helper(
        &mut slot,
        || false,
        || panic!("quit must not authorize"),
        &commands(),
    )
    .unwrap_err();
    assert!(error.message.contains("cleanup denied"));
    assert!(slot.is_some());
    worker.join().unwrap();
}

#[test]
fn explicit_operations_authorize_once_and_reuse_the_channel() {
    let (parent, worker) = helper_channel(2, true);
    let mut slot = None;
    execute_with_helper(&mut slot, || true, || Ok(parent), &commands()).unwrap();
    execute_with_helper(
        &mut slot,
        || true,
        || panic!("reuse the existing helper"),
        &commands(),
    )
    .unwrap();
    worker.join().unwrap();
}

#[test]
fn explicit_operation_replaces_a_disconnected_authorized_channel() {
    let (old, disconnected) = UnixStream::pair().unwrap();
    drop(disconnected);
    let (replacement, worker) = helper_channel(1, true);
    let mut slot = Some(old);
    execute_with_helper(&mut slot, || true, || Ok(replacement), &commands()).unwrap();
    assert!(slot.is_some());
    worker.join().unwrap();
}

#[test]
fn privileged_helper_accepts_only_fixed_proxy_mutations() {
    assert!(validate_networksetup_command(&[
        "-setwebproxy".into(),
        "Wi-Fi".into(),
        "127.0.0.1".into(),
        "7890".into(),
    ])
    .is_ok());
    assert!(validate_networksetup_command(&[
        "-setwebproxystate".into(),
        "Wi-Fi".into(),
        "off".into(),
    ])
    .is_ok());
    assert!(validate_networksetup_command(&[
        "-setproxybypassdomains".into(),
        "USB LAN".into(),
        "192.168.*".into(),
        "*.example.org".into(),
    ])
    .is_ok());
    assert!(validate_networksetup_command(&[
        "-setproxybypassdomains".into(),
        "Wi-Fi".into(),
        "Empty".into(),
    ])
    .is_ok());
    assert!(
        validate_networksetup_command(&["-setproxybypassdomains".into(), "Wi-Fi".into(),]).is_err()
    );
    assert!(validate_networksetup_command(&[
        "-setproxybypassdomains".into(),
        "Wi-Fi".into(),
        "localhost\ninvalid".into(),
    ])
    .is_err());
    assert!(validate_networksetup_command(&["-listallnetworkservices".into()]).is_err());
    assert!(validate_networksetup_command(&["-setwebproxy".into(), "Wi-Fi".into(),]).is_err());
}

#[test]
fn authorization_script_quotes_every_helper_argument() {
    assert!(PRIVILEGED_COMMAND_SCRIPT.contains("quoted form of"));
    assert!(PRIVILEGED_COMMAND_SCRIPT.contains("with administrator privileges"));
}

#[test]
fn one_authorized_channel_serves_repeated_proxy_transactions() {
    let (mut parent, helper) = UnixStream::pair().expect("create helper channel");
    let server = thread::spawn(move || {
        let mut helper = helper;
        for expected_state in ["on", "off"] {
            let mut line = String::new();
            BufReader::new(&mut helper)
                .read_line(&mut line)
                .expect("read helper request");
            let request: HelperRequest = serde_json::from_str(&line).expect("parse request");
            let HelperRequest::NetworkSetup { commands } = request else {
                panic!("expected proxy request")
            };
            assert_eq!(commands[0][2], expected_state);

            serde_json::to_writer(
                &mut helper,
                &HelperResponse {
                    success: true,
                    code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
            )
            .expect("write helper response");
            helper.write_all(b"\n").expect("terminate response");
            helper.flush().expect("flush response");
        }
    });

    for state in ["on", "off"] {
        let commands = vec![vec![
            "-setwebproxystate".into(),
            "Wi-Fi".into(),
            state.into(),
        ]];
        assert!(
            request(&mut parent, &commands)
                .expect("request proxy transaction")
                .success
        );
    }
    server.join().expect("helper server");
}
