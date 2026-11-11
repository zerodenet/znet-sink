//! Windows-native tests: real process handles, private jobs and explicit I/O.
use super::*;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Stdio,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    },
};

fn command(mode: &str, address: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "runtime_host::child::tests::process_fixture",
            "--nocapture",
        ])
        .env("ZNET_JOB_TEST_MODE", mode)
        .env("ZNET_JOB_TEST_CONTROL", address)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

#[test]
fn process_fixture() {
    let Ok(mode) = std::env::var("ZNET_JOB_TEST_MODE") else {
        return;
    };
    let address = std::env::var("ZNET_JOB_TEST_CONTROL").unwrap();
    if mode == "args" {
        let arguments: Vec<_> = std::env::args_os().collect();
        let separator = arguments.iter().position(|arg| arg == "--").unwrap();
        let received: Vec<_> = arguments[separator + 1..]
            .iter()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(received, round_trip_arguments());
        assert_eq!(
            std::env::var("ZNET_JOB_TEST_EXTRA").unwrap(),
            "中文 value with space"
        );
        assert_eq!(std::env::var("ZNET_Ä").unwrap(), "updated");
    }
    if mode == "root" {
        // The controlling parent releases this real launch gate only after
        // the root is admitted to its job, so the leaf must inherit that job.
        let mut go = [0];
        std::io::stdin().read_exact(&mut go).unwrap();
        assert_eq!(&go, b"G");
        let _leaf = command("leaf", &address).spawn().unwrap();
    }
    let _root = if mode == "owner" {
        let mut child = spawn(&mut command("root", &address)).unwrap();
        child.stdin.as_mut().unwrap().write_all(b"G").unwrap();
        Some(child)
    } else {
        None
    };
    let mut stream = TcpStream::connect(address).unwrap();
    writeln!(stream, "{mode} {}", std::process::id()).unwrap();
    if mode == "leaf" {
        let mut byte = [0];
        while stream.read_exact(&mut byte).is_ok() {
            stream.write_all(&byte).unwrap();
        }
    } else {
        std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
    }
    // Deliberately bypass Rust Drop to model abrupt native process exit.
    std::process::exit(0);
}

struct Peer {
    process: OwnedHandle,
    stream: TcpStream,
}
impl Peer {
    fn wait_exited(&self) {
        assert_eq!(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 5000) },
            WAIT_OBJECT_0
        );
    }
    fn ping(&mut self) {
        self.stream.write_all(b"p").unwrap();
        let mut reply = [0];
        self.stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"p");
    }
}

fn peers(listener: &TcpListener, count: usize) -> BTreeMap<String, Peer> {
    listener.set_nonblocking(true).unwrap();
    let mut peers = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while peers.len() < count {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("fixture control accept: {error}"),
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(&mut stream).read_line(&mut line).unwrap();
        let (role, pid) = line.trim().split_once(' ').unwrap();
        // Pin the actual live native process object before any parent is killed.
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid.parse().unwrap(),
            )
        };
        assert!(!raw.is_null(), "{}", io::Error::last_os_error());
        assert!(peers
            .insert(
                role.to_owned(),
                Peer {
                    process: unsafe { OwnedHandle::from_raw_handle(raw) },
                    stream,
                }
            )
            .is_none());
    }
    peers
}

#[test]
fn root_exit_retires_the_actual_descendant_job_before_replacement() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut root = spawn(&mut command("root", &address)).unwrap();
    root.stdin.as_mut().unwrap().write_all(b"G").unwrap();
    let mut members = peers(&listener, 2);
    members.get_mut("leaf").unwrap().ping();
    root.stdin.take();
    assert!(root.wait().unwrap().success());
    assert!(root.try_wait().unwrap().is_some());
    members["root"].wait_exited();
    members["leaf"].wait_exited();
}

struct Original(std::process::Child);
impl Drop for Original {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn round_trip_arguments() -> Vec<&'static str> {
    vec!["", "a b", r#"a\"b"#, r"C:\path with space\", "中文路径"]
}

#[test]
fn native_creation_round_trips_arguments_and_case_insensitive_unicode_environment() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut command = command("args", &address);
    command
        .arg("--")
        .args(round_trip_arguments())
        .env("ZNET_JOB_TEST_EXTRA", "中文 value with space")
        .env("ZNET_Ä", "old")
        .env("znet_ä", "updated");
    let mut child = spawn(&mut command).unwrap();
    let members = peers(&listener, 1);
    child.stdin.take();
    assert!(child.wait().unwrap().success());
    members["args"].wait_exited();
}

#[test]
fn force_killing_the_gui_owner_closes_its_job_without_killing_an_external_process() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut owner = Original(command("owner", &address).spawn().unwrap());
    let mut members = peers(&listener, 3);
    members.get_mut("leaf").unwrap().ping();
    let external_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let external_address = external_listener.local_addr().unwrap().to_string();
    let mut external = spawn(&mut command("leaf", &external_address)).unwrap();
    let mut outsider = peers(&external_listener, 1);
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    for peer in members.values() {
        peer.wait_exited();
    }
    outsider.get_mut("leaf").unwrap().ping();
    assert!(external.try_wait().unwrap().is_none());
    external.kill().unwrap();
    external.wait().unwrap();
    outsider["leaf"].wait_exited();
}
