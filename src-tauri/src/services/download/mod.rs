//! Persistent, bounded HTTP downloads shared by kernel and application updates.
mod cache;
#[cfg(test)]
mod tests;
mod transfer;

use crate::errors::{AppError, AppResult};
use cache::Cache;
use reqwest::blocking::Client;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_BYTES: u64 = 512 * 1024 * 1024;

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
    client: &Client,
    url: &str,
    identity: &str,
    progress: impl FnMut(Progress),
) -> AppResult<Download> {
    fetch_in(
        &super::data_dir()?.join("downloads"),
        client,
        url,
        identity,
        progress,
        Duration::from_secs(1),
    )
}

fn fetch_in(
    root: &Path,
    client: &Client,
    url: &str,
    identity: &str,
    mut progress: impl FnMut(Progress),
    delay: Duration,
) -> AppResult<Download> {
    let mut cache = Cache::open(root, url, identity)?;
    let mut last_error = String::new();
    for attempt in 1..=4 {
        match transfer::attempt(&mut cache, client, url, attempt, &mut progress) {
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
