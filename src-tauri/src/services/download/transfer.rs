use super::{
    cache::{Cache, Metadata},
    NetworkOptions, Progress, MAX_BYTES,
};
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};
use znet_client_capabilities::network::{self, StreamHead, StreamRequest};
use znet_client_core::capability::Lease;

pub(super) struct Failure {
    pub message: String,
    pub retry: bool,
    pub wait: Option<Duration>,
}
fn fatal(message: impl ToString) -> Failure {
    Failure {
        message: message.to_string(),
        retry: false,
        wait: None,
    }
}
fn retry(message: impl ToString) -> Failure {
    Failure {
        message: message.to_string(),
        retry: true,
        wait: None,
    }
}

pub(super) fn attempt(
    cache: &mut Cache,
    lease: &Lease,
    url: &str,
    max_bytes: u64,
    options: &NetworkOptions,
    attempt: usize,
    progress: &mut impl FnMut(Progress),
) -> Result<(), Failure> {
    let mut offset = cache.len().map_err(|e| fatal(e.message))?;
    if offset > max_bytes {
        cache.reset().map_err(|e| fatal(e.message))?;
        return Err(fatal("下载缓存超过大小限制"));
    }
    if cache.meta.complete && cache.meta.total == Some(offset) && offset > 0 {
        progress(Progress {
            bytes_downloaded: offset,
            bytes_total: Some(offset),
            attempt,
            state: "verifying",
        });
        return Ok(());
    }
    // A strong validator prevents concatenating different representations.
    if offset > 0
        && (cache.meta.etag.is_none() || cache.meta.total.is_some_and(|total| offset > total))
    {
        cache.reset().map_err(|e| fatal(e.message))?;
        cache.meta = Metadata::default();
        offset = 0;
    }
    let mut headers = options.headers.clone();
    headers.insert("accept-encoding".into(), "identity".into());
    if offset > 0 {
        headers.insert("range".into(), format!("bytes={offset}-"));
        headers.insert(
            "if-range".into(),
            cache.meta.etag.as_deref().unwrap().to_owned(),
        );
    }
    let mut prepared = None;
    let stream_base = Cell::new(offset);
    let stream_total = Cell::new(cache.meta.total);
    let last_progress = Cell::new(Instant::now());
    let response = network::stream_get(
        lease,
        &StreamRequest {
            url: url.into(),
            user_agent: options.user_agent.clone(),
            headers,
            // Preserve one response byte of budget when a complete partial
            // file needs a final Range probe. A 416 response has no body and
            // confirms the cached artifact without restarting the download.
            max_bytes: max_bytes.saturating_sub(offset).max(1),
            max_redirects: options.max_redirects,
            connect_timeout: options.connect_timeout,
            proxy: options.proxy.clone(),
            no_proxy: options.no_proxy,
            allowed_https_hosts: options.allowed_https_hosts.clone(),
        },
        |head| {
            if head.status == 200 {
                stream_base.set(0);
            }
            let sink = prepare_sink(cache, head, offset, max_bytes, &mut prepared)?;
            stream_total.set(cache.meta.total);
            Ok(sink)
        },
        |written| {
            if last_progress.get().elapsed() >= Duration::from_millis(150) {
                progress(Progress {
                    bytes_downloaded: stream_base.get().saturating_add(written),
                    bytes_total: stream_total.get(),
                    attempt,
                    state: "downloading",
                });
                last_progress.set(Instant::now());
            }
        },
    )
    .and_then(|resource| resource.take(lease))
    .map_err(|error| {
        prepared.take().unwrap_or_else(|| match error {
            znet_client_core::capability::Error::BudgetExceeded => {
                fatal("下载文件为空或超过允许的大小限制")
            }
            znet_client_core::capability::Error::Deadline
            | znet_client_core::capability::Error::Expired => retry("下载超时"),
            znet_client_core::capability::Error::PermissionDenied
            | znet_client_core::capability::Error::InvalidRequest => {
                fatal("下载地址或重定向目标被访问策略拒绝")
            }
            _ => retry("下载网络连接中断"),
        })
    })?;
    let status = response.head.status;
    if status == 416 {
        // A crash may leave every byte on disk before the completion marker.
        let total = response
            .head
            .headers
            .get("content-range")
            .map(String::as_str)
            .and_then(|v| v.strip_prefix("bytes */"))
            .and_then(|n| n.parse().ok());
        let etag = response.head.headers.get("etag").map(String::as_str);
        if offset > 0 && total == Some(offset) && etag == cache.meta.etag.as_deref() {
            cache.meta.total = total;
            cache.meta.complete = true;
            cache.save().map_err(|e| fatal(e.message))?;
            return Ok(());
        }
        cache.reset().map_err(|e| fatal(e.message))?;
        cache.meta = Metadata::default();
        return Err(retry("服务器文件范围发生变化，重新下载"));
    }
    if !(200..300).contains(&status) {
        let mut error = if status >= 500 || matches!(status, 408 | 429) {
            retry(format!("HTTP {status}"))
        } else {
            fatal(format!("下载失败：HTTP {status}"))
        };
        error.wait = response
            .head
            .headers
            .get("retry-after")
            .map(String::as_str)
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);
        return Err(error);
    }
    offset = stream_base.get();
    let total = cache.meta.total;
    progress(Progress {
        bytes_downloaded: offset,
        bytes_total: total,
        attempt,
        state: "downloading",
    });
    let bytes = offset.saturating_add(response.bytes_written);
    if total.is_some_and(|n| n != bytes) {
        return Err(retry("下载连接提前结束"));
    }
    if bytes == 0 {
        return Err(fatal("下载文件为空"));
    }
    cache.meta.total = Some(bytes);
    cache.meta.complete = true;
    cache.save().map_err(|e| fatal(e.message))?;
    progress(Progress {
        bytes_downloaded: bytes,
        bytes_total: Some(bytes),
        attempt,
        state: "verifying",
    });
    Ok(())
}

fn prepare_sink(
    cache: &mut Cache,
    head: &StreamHead,
    offset: u64,
    max_bytes: u64,
    failure: &mut Option<Failure>,
) -> std::io::Result<Option<Box<dyn Write>>> {
    let fail = |reason: Failure, failure: &mut Option<Failure>| {
        *failure = Some(reason);
        std::io::Error::other("download response rejected")
    };
    if head.status == 416 || !(200..300).contains(&head.status) {
        return Ok(None);
    }
    if head
        .headers
        .get("content-encoding")
        .is_some_and(|value| value != "identity")
    {
        return Err(fail(fatal("下载服务器返回了不支持的内容编码"), failure));
    }
    let etag = head
        .headers
        .get("etag")
        .filter(|value| value.starts_with('"') && value.ends_with('"'))
        .cloned();
    let (append, total) = if head.status == 206 {
        let Some((start, end, size)) = head
            .headers
            .get("content-range")
            .and_then(|value| parse_range(value))
        else {
            cache
                .reset()
                .map_err(|_| fail(fatal("下载缓存无法重置"), failure))?;
            cache.meta = Metadata::default();
            return Err(fail(fatal("服务器返回了无效的下载范围"), failure));
        };
        if start != offset
            || head.content_length.is_some_and(|n| n != end - start + 1)
            || (offset > 0
                && (etag != cache.meta.etag || cache.meta.total.is_some_and(|old| old != size)))
        {
            cache
                .reset()
                .map_err(|_| fail(fatal("下载缓存无法重置"), failure))?;
            cache.meta = Metadata::default();
            return Err(fail(retry("服务器文件已变化，重新下载"), failure));
        }
        (true, Some(size))
    } else if head.status == 200 {
        (false, head.content_length)
    } else {
        return Err(fail(
            fatal(format!("不支持的下载响应：HTTP {}", head.status)),
            failure,
        ));
    };
    if total.is_some_and(|size| size == 0 || size > max_bytes) {
        return Err(fail(
            fatal(if max_bytes == MAX_BYTES {
                "下载文件为空或超过 512 MB 限制"
            } else {
                "下载文件为空或超过允许的大小限制"
            }),
            failure,
        ));
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!append)
        .append(append)
        .open(&cache.part)
        .map_err(|error| {
            fail(
                fatal(format!("下载缓存写入失败（请检查磁盘空间）：{error}")),
                failure,
            )
        })?;
    cache.meta = Metadata {
        etag,
        total,
        complete: false,
    };
    cache.save().map_err(|error| {
        fail(
            fatal(format!("下载缓存状态写入失败：{}", error.message)),
            failure,
        )
    })?;
    Ok(Some(Box::new(file)))
}

fn parse_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    (start <= end && end < total).then_some((start, end, total))
}
