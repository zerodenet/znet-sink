use reqwest::{blocking::Client, redirect::Policy, Url};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    time::Duration,
};
use znet_client_core::capability::{Error, Lease, Permission, Resource};

pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_STREAM_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Default)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
}

pub struct StreamRequest {
    pub url: String,
    pub user_agent: String,
    pub headers: BTreeMap<String, String>,
    pub max_bytes: u64,
    pub max_redirects: usize,
    pub connect_timeout: Duration,
    pub proxy: Option<String>,
    pub no_proxy: bool,
    /// Empty means any valid HTTP(S) host. A leading dot matches subdomains.
    pub allowed_https_hosts: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StreamHead {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub content_length: Option<u64>,
}

pub struct StreamResponse {
    pub head: StreamHead,
    pub bytes_written: u64,
}

/// Stream a bounded GET into a sink chosen after response headers are known.
/// URL validation, redirects, proxy construction, deadline checks and byte
/// accounting stay inside the shared executor. Native cache code only decides
/// whether a successful response replaces or appends its local partial file.
pub fn stream_get(
    lease: &Lease,
    input: &StreamRequest,
    mut sink: impl FnMut(&StreamHead) -> std::io::Result<Option<Box<dyn Write>>>,
    mut progress: impl FnMut(u64),
) -> Result<Resource<StreamResponse>, Error> {
    if input.max_bytes == 0
        || input.max_bytes > MAX_STREAM_BYTES
        || input.max_redirects > 10
        || input.connect_timeout.is_zero()
        || input.connect_timeout > Duration::from_secs(60)
    {
        return Err(Error::BudgetExceeded);
    }
    let mut url = parse(&input.url)?;
    check_stream_host(&url, &input.allowed_https_hosts)?;
    let permission = Permission::new("network.download", url.origin().ascii_serialization());
    lease.execute(&permission, || {
        let mut request_headers = checked_headers(&input.headers)?;
        let mut builder = Client::builder()
            .redirect(Policy::none())
            .user_agent(&input.user_agent)
            .connect_timeout(input.connect_timeout);
        if input.no_proxy {
            builder = builder.no_proxy();
        } else if let Some(proxy) = &input.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| Error::InvalidRequest)?);
        }
        let client = builder.build().map_err(|_| Error::InvalidRequest)?;
        for hop in 0..=input.max_redirects {
            lease.check(Some(&Permission::new(
                "network.download",
                url.origin().ascii_serialization(),
            )))?;
            let response = client
                .get(url.clone())
                .headers(request_headers.clone())
                .timeout(lease.remaining()?)
                .send()
                .map_err(|_| Error::Transport)?;
            lease.check(None)?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                if hop == input.max_redirects {
                    return Err(Error::BudgetExceeded);
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .ok_or(Error::InvalidRequest)?;
                let next = parse(
                    url.join(location)
                        .map_err(|_| Error::InvalidRequest)?
                        .as_str(),
                )?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    return Err(Error::PermissionDenied);
                }
                check_stream_host(&next, &input.allowed_https_hosts)?;
                if next.origin() != url.origin() {
                    for name in ["authorization", "cookie", "proxy-authorization"] {
                        request_headers.remove(name);
                    }
                }
                url = next;
                continue;
            }
            let head = StreamHead {
                status: response.status().as_u16(),
                content_length: response.content_length(),
                headers: selected_headers(
                    response.headers(),
                    &["content-encoding", "content-range", "etag", "retry-after"],
                ),
            };
            let Some(mut output) = sink(&head).map_err(|_| Error::Transport)? else {
                return Ok((
                    StreamResponse {
                        head,
                        bytes_written: 0,
                    },
                    0,
                ));
            };
            if head
                .content_length
                .is_some_and(|size| size > input.max_bytes)
            {
                return Err(Error::BudgetExceeded);
            }
            let mut response = response;
            let mut bytes = 0_u64;
            let mut chunk = [0_u8; 64 * 1024];
            loop {
                lease.check(None)?;
                let count = response.read(&mut chunk).map_err(|_| Error::Transport)?;
                if count == 0 {
                    break;
                }
                bytes = bytes.saturating_add(count as u64);
                if bytes > input.max_bytes {
                    return Err(Error::BudgetExceeded);
                }
                output
                    .write_all(&chunk[..count])
                    .map_err(|_| Error::Transport)?;
                progress(bytes);
            }
            output.flush().map_err(|_| Error::Transport)?;
            if head.content_length.is_some_and(|size| size != bytes) {
                return Err(Error::Transport);
            }
            return Ok((
                StreamResponse {
                    head,
                    bytes_written: bytes,
                },
                usize::try_from(bytes).map_err(|_| Error::BudgetExceeded)?,
            ));
        }
        Err(Error::BudgetExceeded)
    })
}

pub fn is_https_host_url(url: &str, host: &str) -> bool {
    parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some(host)
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn check_stream_host(url: &Url, allowed: &[String]) -> Result<(), Error> {
    if allowed.is_empty() {
        return Ok(());
    }
    let host = url.host_str().ok_or(Error::InvalidRequest)?;
    if url.scheme() != "https"
        || url.port().is_some()
        || !allowed.iter().any(|entry| {
            entry.strip_prefix('.').map_or(host == entry, |suffix| {
                host.ends_with(&format!(".{suffix}"))
            })
        })
    {
        return Err(Error::PermissionDenied);
    }
    Ok(())
}

fn checked_headers(
    headers: &BTreeMap<String, String>,
) -> Result<reqwest::header::HeaderMap, Error> {
    if headers.len() > 24
        || headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            > 16 * 1024
    {
        return Err(Error::BudgetExceeded);
    }
    let mut result = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::InvalidRequest)?;
        if matches!(
            name.as_str(),
            "host"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "proxy-connection"
                | "upgrade"
                | "te"
                | "trailer"
        ) {
            return Err(Error::PermissionDenied);
        }
        result.insert(
            name,
            reqwest::header::HeaderValue::from_str(value).map_err(|_| Error::InvalidRequest)?,
        );
    }
    Ok(result)
}

fn selected_headers(
    headers: &reqwest::header::HeaderMap,
    names: &[&str],
) -> BTreeMap<String, String> {
    names
        .iter()
        .filter_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(|value| ((*name).to_owned(), value.to_owned()))
        })
        .collect()
}
/// Canonical scope at the service boundary. Reject implicit URL credentials.
pub fn origin(url: &str) -> Result<String, Error> {
    let url = parse(url)?;
    Ok(url.origin().ascii_serialization())
}
fn parse(url: &str) -> Result<Url, Error> {
    if url.len() > 8192 {
        return Err(Error::InvalidRequest);
    }
    let url = Url::parse(url).map_err(|_| Error::InvalidRequest)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::InvalidRequest);
    }
    Ok(url)
}
/// The only HTTP executor for migrated callers. Every redirect is checked before
/// sending; transport time is bounded by the shared lease. No URLs/body in errors.
pub fn get(
    lease: &Lease,
    url: &str,
    user_agent: &str,
    max_bytes: usize,
    response_headers: &[&str],
) -> Result<Resource<Response>, Error> {
    get_conditional(
        lease,
        url,
        user_agent,
        max_bytes,
        response_headers,
        &Validators::default(),
    )
}

pub fn get_conditional(
    lease: &Lease,
    url: &str,
    user_agent: &str,
    max_bytes: usize,
    response_headers: &[&str],
    validators: &Validators,
) -> Result<Resource<Response>, Error> {
    let mut headers = BTreeMap::new();
    if let Some(value) = &validators.etag {
        headers.insert("if-none-match".into(), value.clone());
    }
    if let Some(value) = &validators.last_modified {
        headers.insert("if-modified-since".into(), value.clone());
    }
    exchange(
        lease,
        &Request {
            method: "GET".into(),
            url: url.into(),
            headers,
            body: Vec::new(),
        },
        Options {
            capability: "network.get",
            user_agent,
            max_bytes,
            response_headers,
            follow: true,
        },
    )
}

/// General HTTP request. Redirect responses are returned to the caller so a
/// credential/body can never be forwarded automatically to another origin.
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
pub fn request(
    lease: &Lease,
    request: &Request,
    max_bytes: usize,
) -> Result<Resource<Response>, Error> {
    exchange(
        lease,
        request,
        Options {
            capability: "network.request",
            user_agent: "ZNet-Sink-Plugin/1",
            max_bytes,
            response_headers: &["content-type", "location"],
            follow: false,
        },
    )
}
struct Options<'a> {
    capability: &'a str,
    user_agent: &'a str,
    max_bytes: usize,
    response_headers: &'a [&'a str],
    follow: bool,
}
fn exchange(
    lease: &Lease,
    input: &Request,
    options: Options<'_>,
) -> Result<Resource<Response>, Error> {
    let Options {
        capability,
        user_agent,
        max_bytes,
        response_headers,
        follow,
    } = options;
    let mut url = parse(&input.url)?;
    let permission = Permission::new(capability, url.origin().ascii_serialization());
    lease.execute(&permission, || {
        if max_bytes == 0 || max_bytes > MAX_RESOURCE_BYTES || input.body.len() > MAX_BODY_BYTES {
            return Err(Error::BudgetExceeded);
        }
        if !matches!(
            input.method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
        ) || (matches!(input.method.as_str(), "GET" | "HEAD") && !input.body.is_empty())
        {
            return Err(Error::InvalidRequest);
        }
        let mut request_headers = reqwest::header::HeaderMap::new();
        if input.headers.len() > 16
            || input
                .headers
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                > 8192
        {
            return Err(Error::BudgetExceeded);
        }
        for (name, value) in &input.headers {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Error::InvalidRequest)?;
            if matches!(
                name.as_str(),
                "host"
                    | "content-length"
                    | "connection"
                    | "transfer-encoding"
                    | "proxy-authorization"
                    | "proxy-connection"
                    | "upgrade"
                    | "te"
                    | "trailer"
            ) {
                return Err(Error::PermissionDenied);
            }
            if request_headers.contains_key(&name) {
                return Err(Error::InvalidRequest);
            }
            request_headers.insert(
                name,
                reqwest::header::HeaderValue::from_str(value).map_err(|_| Error::InvalidRequest)?,
            );
        }
        if response_headers.len() > 8
            || response_headers
                .iter()
                .any(|name| reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err())
        {
            return Err(Error::InvalidRequest);
        }
        let client = Client::builder()
            .redirect(Policy::none())
            .user_agent(user_agent)
            .build()
            .map_err(|_| Error::InvalidRequest)?;
        for hop in 0..=10 {
            lease.check(Some(&Permission::new(
                capability,
                url.origin().ascii_serialization(),
            )))?;
            let method = reqwest::Method::from_bytes(input.method.as_bytes())
                .map_err(|_| Error::InvalidRequest)?;
            let mut response = client
                .request(method, url.clone())
                .headers(request_headers.clone())
                .body(input.body.clone())
                .timeout(lease.remaining()?)
                .send()
                .map_err(|_| Error::Transport)?;
            lease.check(None)?;
            if follow && matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                if hop == 10 {
                    return Err(Error::BudgetExceeded);
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(Error::InvalidRequest)?;
                let next = url.join(location).map_err(|_| Error::InvalidRequest)?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    return Err(Error::PermissionDenied);
                }
                // Builtin conditional GET validators are origin-local metadata.
                if next.origin() != url.origin() {
                    request_headers.clear();
                }
                url = parse(next.as_str())?;
                continue;
            }
            let status = response.status().as_u16();
            if status != 304
                && input.method != "HEAD"
                && response
                    .content_length()
                    .is_some_and(|n| n > max_bytes as u64)
            {
                return Err(Error::BudgetExceeded);
            }
            let mut headers = BTreeMap::new();
            let mut header_bytes = 0;
            for name in response_headers {
                if let Some(value) = response.headers().get(*name).and_then(|v| v.to_str().ok()) {
                    header_bytes += name.len() + value.len();
                    if header_bytes > 8192 {
                        return Err(Error::BudgetExceeded);
                    }
                    headers.insert(name.to_ascii_lowercase(), value.to_owned());
                }
            }
            let mut body = Vec::new();
            let mut buffer = [0u8; 8192];
            loop {
                lease.check(None)?;
                let count = response.read(&mut buffer).map_err(|_| Error::Transport)?;
                if count == 0 {
                    break;
                }
                if count > max_bytes.saturating_sub(body.len()) {
                    return Err(Error::BudgetExceeded);
                }
                body.extend_from_slice(&buffer[..count]);
            }
            let bytes = body.len() + header_bytes;
            return Ok((
                Response {
                    status,
                    body,
                    headers,
                },
                bytes,
            ));
        }
        Err(Error::BudgetExceeded)
    })
}
