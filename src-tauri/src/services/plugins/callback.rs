//! One-shot loopback callbacks for generic external-browser authorization.

use super::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

mod binding;
mod query;
use binding::Scope;

const MAX_SESSIONS: usize = 32;
const MAX_OWNER_SESSIONS: usize = 8;

const LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Default)]
pub(super) struct Store {
    sessions: Arc<Mutex<BTreeMap<String, Session>>>,
}

#[derive(Clone)]
struct Session {
    owner: Scope,
    deadline: Instant,
    state: State,
}

#[derive(Clone)]
enum State {
    Pending,
    Completed(BTreeMap<String, String>),
    Failed,
    Expired,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Created {
    session_id: String,
    callback_url: String,
    expires_at_unix_ms: u64,
}

impl Store {
    fn create(&self, owner: Scope, check: &dyn Fn() -> AppResult<()>) -> AppResult<Created> {
        let mut sessions = self.sessions.lock().map_err(|_| unavailable())?;
        check()?;
        let now = Instant::now();
        sessions.retain(|_, session| {
            session.deadline > now && (session.owner.key != owner.key || session.owner == owner)
        });
        if sessions.len() >= MAX_SESSIONS
            || sessions
                .values()
                .filter(|s| s.owner.key == owner.key)
                .count()
                >= MAX_OWNER_SESSIONS
        {
            return Err(AppError {
                code: "plugin_callback_budget",
                message: "浏览器回调会话达到容量限制，请取消旧会话或等待到期".into(),
                details: None,
            });
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| unavailable())?;
        listener.set_nonblocking(true).map_err(|_| unavailable())?;
        let port = listener.local_addr().map_err(|_| unavailable())?.port();
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(|_| unavailable())?;
        let session_id: String = random.iter().map(|value| format!("{value:02x}")).collect();
        if sessions.contains_key(&session_id) {
            return Err(unavailable());
        }
        let expires_at_unix_ms =
            crate::services::common::now_unix_ms().saturating_add(LIFETIME.as_millis() as u64);
        check()?;
        sessions.insert(
            session_id.clone(),
            Session {
                owner,
                deadline: Instant::now() + LIFETIME,
                state: State::Pending,
            },
        );
        let thread_sessions = Arc::clone(&self.sessions);
        let thread_id = session_id.clone();
        if thread::Builder::new()
            .name("plugin-callback".into())
            .spawn(move || accept(listener, thread_sessions, thread_id))
            .is_err()
        {
            sessions.remove(&session_id);
            return Err(unavailable());
        }
        Ok(Created {
            callback_url: format!("http://127.0.0.1:{port}/callback/{session_id}"),
            session_id,
            expires_at_unix_ms,
        })
    }

    fn poll(&self, owner: &Scope, id: &str) -> AppResult<serde_json::Value> {
        validate_id(id)?;
        let mut sessions = self.sessions.lock().map_err(|_| unavailable())?;
        let session = sessions
            .get_mut(id)
            .filter(|session| &session.owner == owner)
            .ok_or_else(missing)?;
        if Instant::now() >= session.deadline {
            session.state = State::Expired;
        }
        Ok(match &session.state {
            State::Pending => serde_json::json!({"state":"pending"}),
            State::Completed(query) => serde_json::json!({"state":"completed","query":query}),
            State::Failed => serde_json::json!({"state":"failed"}),
            State::Expired => serde_json::json!({"state":"expired"}),
        })
    }

    fn cancel(&self, owner: &Scope, id: &str, check: &dyn Fn() -> AppResult<()>) -> AppResult<()> {
        validate_id(id)?;
        let mut sessions = self.sessions.lock().map_err(|_| unavailable())?;
        if !sessions
            .get(id)
            .is_some_and(|session| &session.owner == owner)
        {
            return Err(missing());
        }
        check()?;
        sessions.remove(id);
        Ok(())
    }

    pub(super) fn cancel_owner(&self, owner: &str) {
        self.sessions
            .lock()
            .unwrap()
            .retain(|_, session| session.owner.key != owner);
    }

    pub(super) fn cancel_plugin(&self, plugin_id: &str) {
        let prefix = format!("{plugin_id}/");
        self.sessions
            .lock()
            .unwrap()
            .retain(|_, session| !session.owner.key.starts_with(&prefix));
    }
}

fn unavailable() -> AppError {
    AppError {
        code: "plugin_callback_unavailable",
        message: "浏览器回调接收通道暂不可用".into(),
        details: None,
    }
}
fn missing() -> AppError {
    AppError {
        code: "not_found",
        message: "浏览器回调会话不存在、已过期或不属于原授权".into(),
        details: None,
    }
}
fn validate_id(id: &str) -> AppResult<()> {
    if id.len() != 48
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AppError::invalid_argument("浏览器回调会话编号无效"));
    }
    Ok(())
}
fn accept(listener: TcpListener, sessions: Arc<Mutex<BTreeMap<String, Session>>>, id: String) {
    loop {
        let deadline = {
            let Ok(mut sessions) = sessions.lock() else {
                return;
            };
            let Some(session) = sessions.get_mut(&id) else {
                return;
            };
            if Instant::now() >= session.deadline {
                session.state = State::Expired;
                return;
            }
            if !matches!(session.state, State::Pending) {
                return;
            }
            session.deadline
        };
        match listener.accept() {
            Ok((mut stream, address)) if address.ip().is_loopback() => {
                let result = read_callback_until(
                    &mut stream,
                    &id,
                    deadline.min(Instant::now() + Duration::from_secs(2)),
                );
                let accepted = if let Ok(mut entries) = sessions.lock() {
                    if let Some(session) = entries.get_mut(&id).filter(|s| {
                        s.deadline > Instant::now() && matches!(s.state, State::Pending)
                    }) {
                        session.state = match result {
                            Ok(query) => State::Completed(query),
                            Err(_) => State::Failed,
                        };
                        matches!(session.state, State::Completed(_))
                    } else {
                        false
                    }
                } else {
                    false
                };
                let (status, content_type, body) = if accepted {
                    ("200 OK", "text/html; charset=utf-8", "<!doctype html><meta charset=utf-8><title>Callback received</title>Callback received. Return to ZNet Sink to check the result.")
                } else {
                    (
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        "Callback request was not accepted",
                    )
                };
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri 'none'\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let _ = stream.write_all(response.as_bytes());
                return;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50))
            }
            Err(_) => {
                if let Ok(mut sessions) = sessions.lock() {
                    if let Some(session) = sessions.get_mut(&id) {
                        session.state = State::Failed;
                    }
                }
                return;
            }
        }
    }
}

#[cfg(test)]
fn read_callback(stream: &mut TcpStream, id: &str) -> Result<BTreeMap<String, String>, ()> {
    read_callback_until(stream, id, Instant::now() + Duration::from_secs(2))
}
fn read_callback_until(
    stream: &mut TcpStream,
    id: &str,
    deadline: Instant,
) -> Result<BTreeMap<String, String>, ()> {
    // macOS may inherit the listener's nonblocking flag on accepted sockets.
    // Read the complete header with the timeout below, including TCP fragments.
    stream.set_nonblocking(false).map_err(|_| ())?;
    const MAX_REQUEST_BYTES: usize = 8192;
    let mut bytes = Vec::with_capacity(1024);
    while bytes.len() < MAX_REQUEST_BYTES {
        let mut chunk = [0u8; 1024];
        let remaining = MAX_REQUEST_BYTES - bytes.len();
        let read_len = remaining.min(chunk.len());
        let remaining_time = deadline.checked_duration_since(Instant::now()).ok_or(())?;
        stream
            .set_read_timeout(Some(remaining_time.max(Duration::from_millis(1))))
            .map_err(|_| ())?;
        let count = stream.read(&mut chunk[..read_len]).map_err(|_| ())?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        return Err(());
    }
    let request = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let first = request.lines().next().ok_or(())?;
    let mut parts = first.split(' ');
    if parts.next() != Some("GET") {
        return Err(());
    }
    let target = parts.next().ok_or(())?;
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) || parts.next().is_some() {
        return Err(());
    }
    query::parse(target, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_callback_is_owner_scoped() {
        let store = Store::default();
        let created = store
            .create(Scope::fixture("org.example/identity"), &|| Ok(()))
            .unwrap();
        assert!(store
            .poll(&Scope::fixture("org.other/identity"), &created.session_id)
            .is_err());
        let url = reqwest::Url::parse(&created.callback_url).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            stream,
            "GET {}?code=opaque HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            url.path()
        )
        .unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let status = store
                .poll(&Scope::fixture("org.example/identity"), &created.session_id)
                .unwrap();
            if status["state"] == "completed" {
                assert_eq!(status["query"]["code"], "opaque");
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn fragmented_callback_waits_for_complete_headers_on_an_inherited_nonblocking_socket() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .write_all(b"GET /callback/test?code=opaque HTTP/1.1\r\nHost:")
            .unwrap();
        let (mut accepted, _) = listener.accept().unwrap();
        accepted.set_nonblocking(true).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            ready_tx.send(()).unwrap();
            result_tx
                .send(read_callback(&mut accepted, "test"))
                .unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            result_rx.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            "an incomplete header must wait for its remaining bytes"
        );
        client.write_all(b" 127.0.0.1\r\n\r\n").unwrap();
        let query = result_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(query["code"], "opaque");
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod ownership_tests;

#[cfg(test)]
mod budget_tests;
