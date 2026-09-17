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

const LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Default)]
pub(super) struct Store {
    sessions: Arc<Mutex<BTreeMap<String, Session>>>,
}

#[derive(Clone)]
struct Session {
    owner: String,
    expires_at_unix_ms: u64,
    state: State,
}

#[derive(Clone)]
enum State {
    Pending,
    Completed(BTreeMap<String, String>),
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Created {
    session_id: String,
    callback_url: String,
    expires_at_unix_ms: u64,
}

impl Store {
    pub(super) fn create(&self, owner: String) -> AppResult<Created> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(io::failure)?;
        listener.set_nonblocking(true).map_err(io::failure)?;
        let port = listener.local_addr().map_err(io::failure)?.port();
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(io::failure)?;
        let session_id: String = random.iter().map(|value| format!("{value:02x}")).collect();
        let expires_at_unix_ms =
            crate::services::common::now_unix_ms().saturating_add(LIFETIME.as_millis() as u64);
        self.sessions.lock().unwrap().insert(
            session_id.clone(),
            Session {
                owner,
                expires_at_unix_ms,
                state: State::Pending,
            },
        );
        let sessions = Arc::clone(&self.sessions);
        let thread_id = session_id.clone();
        thread::spawn(move || accept(listener, sessions, thread_id));
        Ok(Created {
            callback_url: format!("http://127.0.0.1:{port}/callback/{session_id}"),
            session_id,
            expires_at_unix_ms,
        })
    }

    pub(super) fn poll(&self, owner: &str, id: &str) -> AppResult<serde_json::Value> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(id)
            .filter(|session| session.owner == owner)
            .ok_or_else(|| AppError::not_found("plugin_callback", id))?;
        if crate::services::common::now_unix_ms() >= session.expires_at_unix_ms {
            return Ok(serde_json::json!({"state":"expired"}));
        }
        Ok(match &session.state {
            State::Pending => serde_json::json!({"state":"pending"}),
            State::Completed(query) => serde_json::json!({"state":"completed","query":query}),
            State::Failed => serde_json::json!({"state":"failed"}),
        })
    }

    pub(super) fn cancel(&self, owner: &str, id: &str) -> AppResult<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions
            .get(id)
            .is_some_and(|session| session.owner == owner)
        {
            sessions.remove(id);
            return Ok(());
        }
        Err(AppError::not_found("plugin_callback", id))
    }

    pub(super) fn cancel_owner(&self, owner: &str) {
        self.sessions
            .lock()
            .unwrap()
            .retain(|_, session| session.owner != owner);
    }

    pub(super) fn cancel_plugin(&self, plugin_id: &str) {
        let prefix = format!("{plugin_id}/");
        self.sessions
            .lock()
            .unwrap()
            .retain(|_, session| !session.owner.starts_with(&prefix));
    }
}

fn accept(listener: TcpListener, sessions: Arc<Mutex<BTreeMap<String, Session>>>, id: String) {
    let deadline = Instant::now() + LIFETIME;
    loop {
        if Instant::now() >= deadline || !sessions.lock().unwrap().contains_key(&id) {
            return;
        }
        match listener.accept() {
            Ok((mut stream, address)) if address.ip().is_loopback() => {
                let result = read_callback(&mut stream, &id);
                let (status, content_type, body) = if result.is_ok() {
                    (
                        "200 OK",
                        "text/html; charset=utf-8",
                        "<!doctype html><meta charset=utf-8><title>Completed</title>Authorization completed. Return to ZNet Sink.",
                    )
                } else {
                    (
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        "Invalid callback request",
                    )
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                if let Some(session) = sessions.lock().unwrap().get_mut(&id) {
                    session.state = match result {
                        Ok(query) => State::Completed(query),
                        Err(_) => State::Failed,
                    };
                }
                return;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                if let Some(session) = sessions.lock().unwrap().get_mut(&id) {
                    session.state = State::Failed;
                }
                return;
            }
        }
    }
}

fn read_callback(stream: &mut TcpStream, id: &str) -> Result<BTreeMap<String, String>, ()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| ())?;
    const MAX_REQUEST_BYTES: usize = 8192;
    let mut bytes = Vec::with_capacity(1024);
    while bytes.len() < MAX_REQUEST_BYTES {
        let mut chunk = [0u8; 1024];
        let remaining = MAX_REQUEST_BYTES - bytes.len();
        let read_len = remaining.min(chunk.len());
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
    let target = first
        .strip_prefix("GET ")
        .and_then(|value| value.split_once(' ').map(|pair| pair.0))
        .ok_or(())?;
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| ())?;
    if url.path() != format!("/callback/{id}") {
        return Err(());
    }
    let query: BTreeMap<_, _> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if query.len() > 32
        || query
            .iter()
            .any(|(key, value)| key.len() > 128 || value.len() > 4096)
    {
        return Err(());
    }
    Ok(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_callback_is_owner_scoped() {
        let store = Store::default();
        let created = store.create("org.example/identity".into()).unwrap();
        assert!(store
            .poll("org.other/identity", &created.session_id)
            .is_err());
        let url = reqwest::Url::parse(&created.callback_url).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
        write!(
            stream,
            "GET {}?code=opaque HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            url.path()
        )
        .unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        if !response.is_empty() {
            assert!(response.starts_with("HTTP/1.1 200 OK"));
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let status = store
                .poll("org.example/identity", &created.session_id)
                .unwrap();
            if status["state"] == "completed" {
                assert_eq!(status["query"]["code"], "opaque");
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
}
