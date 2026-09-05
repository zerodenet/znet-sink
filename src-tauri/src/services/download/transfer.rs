use super::{
    cache::{Cache, Metadata},
    Progress, MAX_BYTES,
};
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, RANGE,
    RETRY_AFTER,
};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

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
    client: &Client,
    url: &str,
    attempt: usize,
    progress: &mut impl FnMut(Progress),
) -> Result<(), Failure> {
    let mut offset = cache.len().map_err(|e| fatal(e.message))?;
    if offset > MAX_BYTES {
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
    let mut request = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .timeout(Duration::from_secs(120));
    if offset > 0 {
        request = request
            .header(RANGE, format!("bytes={offset}-"))
            .header(IF_RANGE, cache.meta.etag.as_deref().unwrap());
    }
    let mut response = request.send().map_err(retry)?;
    let status = response.status();
    if status.as_u16() == 416 {
        // A crash may leave every byte on disk before the completion marker.
        let total = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes */"))
            .and_then(|n| n.parse().ok());
        let etag = response.headers().get(ETAG).and_then(|v| v.to_str().ok());
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
    if !status.is_success() {
        let mut error = if status.is_server_error() || matches!(status.as_u16(), 408 | 429) {
            retry(format!("HTTP {status}"))
        } else {
            fatal(format!("下载失败：HTTP {status}"))
        };
        error.wait = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);
        return Err(error);
    }
    if response
        .headers()
        .get(CONTENT_ENCODING)
        .is_some_and(|v| v != "identity")
    {
        return Err(fatal("下载服务器返回了不支持的内容编码"));
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.starts_with('"') && v.ends_with('"'))
        .map(str::to_owned);
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let total;
    if status.as_u16() == 206 {
        let (start, end, size) = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_range)
            .ok_or_else(|| fatal("服务器返回了无效的下载范围"))?;
        if start != offset
            || content_length.is_some_and(|n| n != end - start + 1)
            || (offset > 0
                && (etag != cache.meta.etag || cache.meta.total.is_some_and(|old| old != size)))
        {
            cache.reset().map_err(|e| fatal(e.message))?;
            cache.meta = Metadata::default();
            return Err(retry("服务器文件已变化，重新下载"));
        }
        total = Some(size);
    } else if status.as_u16() == 200 {
        // Range ignored or If-Range no longer matches: replace, never append.
        offset = 0;
        total = content_length;
    } else {
        return Err(fatal(format!("不支持的下载响应：HTTP {status}")));
    }
    if total.is_some_and(|n| n == 0 || n > MAX_BYTES) {
        return Err(fatal("下载文件为空或超过 512 MB 限制"));
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .append(offset > 0)
        .open(&cache.part)
        .map_err(fatal)?;
    cache.meta = Metadata {
        etag,
        total,
        complete: false,
    };
    cache.save().map_err(|e| fatal(e.message))?;
    progress(Progress {
        bytes_downloaded: offset,
        bytes_total: total,
        attempt,
        state: "downloading",
    });
    let mut bytes = offset;
    let mut chunk = [0; 64 * 1024];
    let mut last = Instant::now();
    loop {
        let count = response.read(&mut chunk).map_err(retry)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > MAX_BYTES || total.is_some_and(|n| bytes > n) {
            drop(file);
            cache.reset().map_err(|e| fatal(e.message))?;
            return Err(fatal("下载大小与服务器声明不一致"));
        }
        file.write_all(&chunk[..count])
            .map_err(|e| fatal(format!("下载缓存写入失败（请检查磁盘空间）：{e}")))?;
        if last.elapsed() >= Duration::from_millis(150) {
            progress(Progress {
                bytes_downloaded: bytes,
                bytes_total: total,
                attempt,
                state: "downloading",
            });
            last = Instant::now();
        }
    }
    file.sync_all().map_err(fatal)?;
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

fn parse_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    (start <= end && end < total).then_some((start, end, total))
}
