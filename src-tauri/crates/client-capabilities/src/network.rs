use reqwest::{blocking::Client, redirect::Policy, Url};
use std::{collections::BTreeMap, io::Read};
use znet_client_core::capability::{Error, Lease, Permission, Resource};

pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;

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
