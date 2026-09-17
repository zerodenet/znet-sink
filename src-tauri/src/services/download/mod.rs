//! Persistent, bounded HTTP downloads shared by kernel and application updates.
mod cache;
#[cfg(test)]
mod tests;
mod transfer;

use crate::errors::{AppError, AppResult};
use cache::Cache;
use serde::Serialize;
use std::time::Duration;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use znet_client_core::capability::Manager;

pub const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct NetworkOptions {
    pub user_agent: String,
    pub headers: BTreeMap<String, String>,
    pub proxy: Option<String>,
    pub no_proxy: bool,
    pub connect_timeout: Duration,
    pub timeout: Duration,
    pub max_redirects: usize,
    pub allowed_https_hosts: Vec<String>,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            user_agent: "znet-sink".into(),
            headers: BTreeMap::new(),
            proxy: None,
            no_proxy: false,
            connect_timeout: Duration::from_secs(30),
            timeout: Duration::from_secs(600),
            max_redirects: 10,
            allowed_https_hosts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub bytes_downloaded: u64,
    pub bytes_total: Option<u64>,
    pub attempt: usize,
    pub state: &'static str,
}

pub struct Download {
    pub path: PathBuf,
    // Hold the OS file lock until the caller has validated/consumed the file.
    cache: Cache,
}
impl Download {
    pub fn discard(&self) -> AppResult<()> {
        self.cache.reset()
    }
}

pub fn fetch(
    manager: &Manager,
    url: &str,
    identity: &str,
    options: &NetworkOptions,
    progress: impl FnMut(Progress),
) -> AppResult<Download> {
    fetch_bounded(manager, url, identity, MAX_BYTES, options, progress)
}

pub fn fetch_bounded(
    manager: &Manager,
    url: &str,
    identity: &str,
    max_bytes: u64,
    options: &NetworkOptions,
    progress: impl FnMut(Progress),
) -> AppResult<Download> {
    if max_bytes == 0 || max_bytes > MAX_BYTES {
        return Err(AppError::invalid_argument("下载大小限制无效"));
    }
    fetch_in(
        &super::data_dir()?.join("downloads"),
        manager,
        url,
        identity,
        max_bytes,
        options,
        progress,
        Duration::from_secs(1),
    )
}

fn fetch_in(
    root: &Path,
    manager: &Manager,
    url: &str,
    identity: &str,
    max_bytes: u64,
    options: &NetworkOptions,
    mut progress: impl FnMut(Progress),
    delay: Duration,
) -> AppResult<Download> {
    let mut cache = Cache::open(root, url, identity)?;
    let lease = znet_client_capabilities::host::network(
        manager,
        &format!("builtin.download:{identity}"),
        "network.download",
        4,
        max_bytes as usize,
        options.timeout,
    )
    .map_err(|error| AppError::internal(format!("下载能力不可用：{error}")))?;
    let mut last_error = String::new();
    for attempt in 1..=4 {
        match transfer::attempt(
            &mut cache,
            &lease,
            url,
            max_bytes,
            options,
            attempt,
            &mut progress,
        ) {
            Ok(()) => {
                return Ok(Download {
                    path: cache.part.clone(),
                    cache,
                })
            }
            Err(transfer::Failure {
                message,
                retry,
                wait,
            }) => {
                if !retry {
                    return Err(AppError::internal(message));
                }
                last_error = message;
                if attempt < 4 {
                    progress(Progress {
                        bytes_downloaded: cache.len()?,
                        bytes_total: cache.meta.total,
                        attempt,
                        state: "retrying",
                    });
                    std::thread::sleep(
                        wait.unwrap_or(delay * (1 << (attempt - 1)))
                            .min(Duration::from_secs(30)),
                    );
                }
            }
        }
    }
    Err(AppError::internal(format!(
        "下载中断，已保留进度；重新下载同一版本将继续：{last_error}"
    )))
}

pub fn cached(url: &str, identity: &str) -> AppResult<Download> {
    let cache = Cache::open(&super::data_dir()?.join("downloads"), url, identity)?;
    let len = cache.len()?;
    if !cache.meta.complete || cache.meta.total != Some(len) || len == 0 || len > MAX_BYTES {
        return Err(AppError::internal("下载缓存不完整或已过期，请重新下载"));
    }
    Ok(Download {
        path: cache.part.clone(),
        cache,
    })
}
