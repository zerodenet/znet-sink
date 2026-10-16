//! Read-only release metadata and ordinary local preferences. No updater handles,
//! installer, downloaded code or runtime effects. Foreground scheduling is native-owned.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;
use crate::models::kernel_version::ReleaseChannel;
use crate::models::mobile_release::*;
use crate::state::app_state::AppState;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Duration};

pub(super) const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_RELEASES: usize = 30;
const MAX_NOTES_BYTES: usize = 64 * 1024;

pub(super) fn revision(config: &AppConfig) -> AppResult<String> {
    let bytes =
        serde_json::to_vec(config).map_err(|_| AppError::internal("发行偏好版本读取失败"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn preferences(config: &AppConfig) -> AppResult<ReleasePreferencesSnapshot> {
    Ok(ReleasePreferencesSnapshot {
        preferences: config.release_preferences.clone(),
        expected_revision: revision(config)?,
        scheduler_state: if cfg!(any(target_os = "android", target_os = "ios")) {
            "native-foreground"
        } else {
            "not-native-host"
        },
        operation: None,
    })
}

pub(super) fn candidate(
    current: &AppConfig,
    expected_revision: &str,
    preferences: ReleasePreferences,
) -> AppResult<AppConfig> {
    if revision(current)? != expected_revision {
        return Err(AppError::conflict(
            "releasePreferences",
            "",
            "设置已变化，请重新读取发行偏好",
        ));
    }
    let mut candidate = current.clone();
    candidate.release_preferences = preferences;
    Ok(candidate)
}

fn repository(source: MobileReleaseSource) -> &'static str {
    match source {
        MobileReleaseSource::Client => "znet-sink",
        MobileReleaseSource::Kernel => "core",
    }
}

pub(crate) fn catalog(
    state: &AppState,
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
) -> AppResult<MobileReleaseCatalog> {
    state
        .mobile_release_scheduler()
        .check(state, source, channel, false)
}

pub(super) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
}
struct MemorySink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for MemorySink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut target = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("buffer unavailable"))?;
        if target.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err(std::io::Error::other("buffer budget"));
        }
        target.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn fetch(
    manager: &znet_client_core::capability::Manager,
    source: MobileReleaseSource,
    etag: Option<&str>,
) -> AppResult<Reply> {
    let url = format!(
        "https://api.github.com/repos/zerodenet/{}/releases?per_page={MAX_RELEASES}",
        repository(source)
    );
    fetch_request(manager, url, etag, vec!["api.github.com".into()], false)
}
#[cfg(test)]
pub(super) fn fetch_for_test(
    manager: &znet_client_core::capability::Manager,
    url: String,
    etag: Option<&str>,
) -> AppResult<Reply> {
    fetch_request(manager, url, etag, Vec::new(), true)
}
fn fetch_request(
    manager: &znet_client_core::capability::Manager,
    url: String,
    etag: Option<&str>,
    allowed_https_hosts: Vec<String>,
    no_proxy: bool,
) -> AppResult<Reply> {
    let lease = znet_client_capabilities::host::network(
        manager,
        "builtin.mobile.release-catalog",
        "network.download",
        1,
        MAX_BYTES,
        Duration::from_secs(30),
    )
    .map_err(|_| AppError::internal("发行目录网络权限或预算不可用"))?;
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut headers = BTreeMap::from([("accept".into(), "application/vnd.github+json".into())]);
    if let Some(etag) = etag {
        headers.insert("if-none-match".into(), etag.into());
    }
    let response = znet_client_capabilities::network::stream_get(
        &lease,
        &znet_client_capabilities::network::StreamRequest {
            url,
            user_agent: "ZNet-Sink-release-catalog".into(),
            headers,
            max_bytes: MAX_BYTES as u64,
            max_redirects: 0,
            connect_timeout: Duration::from_secs(10),
            proxy: None,
            no_proxy,
            allowed_https_hosts,
        },
        |head| {
            Ok((head.status == 200)
                .then(|| Box::new(MemorySink(bytes.clone())) as Box<dyn std::io::Write>))
        },
        |_| {},
    )
    .and_then(|r| r.take(&lease))
    .map_err(|_| AppError::internal("发行目录读取失败或超出预算，请重试"))?;
    let body = std::mem::take(
        &mut *bytes
            .lock()
            .map_err(|_| AppError::internal("发行目录缓冲不可用"))?,
    );
    let etag = response
        .head
        .headers
        .get("etag")
        .filter(|value| {
            !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        })
        .cloned();
    Ok(Reply {
        status: response.head.status,
        body,
        etag,
    })
}
pub(super) fn build_catalog(
    bytes: &[u8],
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
    request_id: String,
    response_status: u16,
    checked_at: u64,
) -> AppResult<MobileReleaseCatalog> {
    let releases = parse_catalog(bytes, source, channel)?;
    Ok(MobileReleaseCatalog {
        source,
        channel,
        platform: crate::mobile_host::snapshot().platform,
        checked_at_unix_ms: checked_at.to_string(),
        releases,
        catalog_limit: MAX_RELEASES,
        target_status: if source == MobileReleaseSource::Kernel {
            "bundled-via-client"
        } else {
            "installation-not-adapted"
        },
        target_reason: if source == MobileReleaseSource::Kernel {
            "默认内核随应用更新；独立移动内核安装不开放。目录版本不代表已接入随包产物。"
        } else {
            "当前目标的移动安装器及发行清单尚未适配；发行版本或资产扩展名不能确认手机安装兼容性。"
        },
        request_id,
        response_status,
        reused_validated_cache: response_status == 304,
    })
}

fn parse_catalog(
    bytes: &[u8],
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
) -> AppResult<Vec<MobileRelease>> {
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("发行目录超出读取预算"));
    }
    let payload: Vec<Value> =
        serde_json::from_slice(bytes).map_err(|_| AppError::internal("发行目录格式无效"))?;
    if payload.len() > MAX_RELEASES {
        return Err(AppError::internal("发行目录超出条目预算"));
    }
    if payload.iter().any(|raw| {
        !raw.is_object()
            || raw.get("draft").and_then(Value::as_bool).is_none()
            || raw
                .get("tag_name")
                .and_then(Value::as_str)
                .is_none_or(|tag| tag.len() > 256)
    }) {
        return Err(AppError::internal("发行目录条目格式无效"));
    }
    let mut releases: Vec<(Version, MobileRelease)> = payload
        .iter()
        .filter_map(|raw| parse_release(raw, source))
        .filter(|(_, release)| release.channel == channel)
        .collect();
    releases.sort_by(|(a, _), (b, _)| b.cmp(a));
    Ok(releases.into_iter().map(|(_, release)| release).collect())
}

fn parse_release(raw: &Value, source: MobileReleaseSource) -> Option<(Version, MobileRelease)> {
    if raw.get("draft").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    let tag = raw.get("tag_name")?.as_str()?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    if tag != version.to_string() && tag != format!("v{version}") {
        return None;
    }
    let (channel, kernel_channel) = if source == MobileReleaseSource::Kernel {
        let original = super::kernel_manager::classify_channel(
            tag,
            raw.get("prerelease")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
        let projected = match original {
            ReleaseChannel::Stable => MobileReleaseChannel::Stable,
            ReleaseChannel::Beta => MobileReleaseChannel::Preview,
            ReleaseChannel::Nightly => MobileReleaseChannel::Test,
        };
        (projected, Some(original))
    } else {
        // Exact existing app policy: rc/preview are candidates, other prereleases test.
        let first = version
            .pre
            .as_str()
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        (
            if version.pre.is_empty() {
                MobileReleaseChannel::Stable
            } else if first == "rc" || first == "preview" {
                MobileReleaseChannel::Preview
            } else {
                MobileReleaseChannel::Test
            },
            None,
        )
    };
    let mut notes = raw
        .get("body")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned);
    let notes_truncated = notes.as_ref().is_some_and(|v| v.len() > MAX_NOTES_BYTES);
    if let Some(notes) = notes.as_mut().filter(|_| notes_truncated) {
        let mut boundary = MAX_NOTES_BYTES;
        while !notes.is_char_boundary(boundary) {
            boundary -= 1;
        }
        notes.truncate(boundary);
    }
    Some((
        version.clone(),
        MobileRelease {
            tag_name: tag.into(),
            version: version.to_string(),
            channel,
            kernel_channel,
            published_at: raw
                .get("published_at")
                .and_then(Value::as_str)
                .filter(|v| v.len() < 128)
                .map(str::to_owned),
            notes,
            notes_truncated,
            // Treat upstream notes as text; external links use canonical trusted project URL.
            release_url: format!(
                "https://github.com/zerodenet/{}/releases/tag/{tag}",
                repository(source)
            ),
        },
    ))
}

#[cfg(test)]
mod tests;
