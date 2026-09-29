use super::{exchange_diagnosed, Options, Request, Response, Route};
use std::{fmt, io, time::Duration};
use znet_client_core::capability::{Error, Lease, Resource};

/// Safe classifications only: never expose URLs, credentials or response bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFailure {
    Timeout,
    Connect,
    ResponseBody,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestFailure {
    pub error: Error,
    pub transport: Option<TransportFailure>,
}

impl fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.transport {
            Some(TransportFailure::Timeout) => f.write_str("请求超时"),
            Some(TransportFailure::Connect) => f.write_str("建立连接失败（DNS、TCP 或 TLS 阶段）"),
            Some(TransportFailure::ResponseBody) => f.write_str("响应读取失败"),
            Some(TransportFailure::Other) => f.write_str("网络传输失败"),
            None => self.error.fmt(f),
        }
    }
}
impl std::error::Error for RequestFailure {}

pub(super) fn sending(error: &reqwest::Error) -> TransportFailure {
    if error.is_timeout() {
        TransportFailure::Timeout
    } else if error.is_connect() {
        TransportFailure::Connect
    } else if error.is_body() || error.is_decode() {
        TransportFailure::ResponseBody
    } else {
        TransportFailure::Other
    }
}

pub(super) fn reading(error: &io::Error) -> TransportFailure {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) || error
        .get_ref()
        .and_then(|source| source.downcast_ref::<reqwest::Error>())
        .is_some_and(reqwest::Error::is_timeout)
    {
        TransportFailure::Timeout
    } else {
        TransportFailure::ResponseBody
    }
}

/// Same admission, redirects and resource checks as `get`, with a per-attempt
/// timeout inside the lease deadline and an optional safe transport category.
pub fn get_with_diagnostics(
    lease: &Lease,
    url: &str,
    user_agent: &str,
    max_bytes: usize,
    response_headers: &[&str],
    timeout: Duration,
) -> Result<Resource<Response>, RequestFailure> {
    let mut transport = None;
    exchange_diagnosed(
        lease,
        &Request {
            method: "GET".into(),
            url: url.into(),
            headers: Default::default(),
            body: Vec::new(),
        },
        Options {
            capability: "network.get",
            permission_scope: None,
            user_agent,
            max_bytes,
            response_headers,
            follow: true,
            route: Route::System,
        },
        Some(timeout),
        &mut transport,
    )
    .map_err(|error| RequestFailure {
        error,
        // The lease's final delivery check can override a failed request with
        // revocation, cancellation or expiry. Keep that authoritative outcome.
        transport: if error == Error::Transport {
            transport
        } else {
            None
        },
    })
}
