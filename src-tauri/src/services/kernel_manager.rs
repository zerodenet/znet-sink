use std::fs;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppCoreConfig;
use crate::models::kernel_version::{
    KernelDownloadProgress, KernelInstallResult, KernelRelease, ReleaseChannel,
    KernelVersionList, KernelVersionDetect,
};
use super::data_dir;
use crate::services::core_config;

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/zerodenet/zero/releases?per_page=30";
const PROGRESS_EVENT: &str = "kernel:download-progress";
const CHUNK_SIZE: usize = 8 * 1024;
const PROGRESS_INTERVAL: u64 = 64 * 1024;

pub fn list_available_versions() -> AppResult<KernelVersionList> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("znet-sink")
        .build()
        .map_err(|e| AppError::internal(format!("failed to create http client: {e}")))?;

    let mut resp = client
        .get(GITHUB_RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| AppError::internal(format!("failed to fetch releases: {e}")))?;

    let mut body = String::new();
    resp.read_to_string(&mut body)
        .map_err(|e| AppError::internal(format!("failed to read releases response: {e}")))?;

    let releases_json: Vec<serde_json::Value> = serde_json::from_str(&body)
        .map_err(|e| AppError::internal(format!("failed to parse releases: {e}")))?;

    let platform_asset = platform_asset_name();
    let mut versions: Vec<KernelRelease> = releases_json
        .into_iter()
        .filter_map(|release| parse_release(&client, &release, platform_asset))
        .collect();

    versions.sort_by(|a, b| {
        b.published_at_unix_ms
            .unwrap_or(0)
            .cmp(&a.published_at_unix_ms.unwrap_or(0))
    });

    Ok(KernelVersionList { versions })
}

pub fn install_version(
    version: String,
    download_url: String,
    expected_sha256: Option<String>,
    install_dir: Option<String>,
    app: AppHandle,
) -> AppResult<KernelInstallResult> {
    let dir = resolve_install_dir(install_dir)?;
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::internal(format!("failed to create install dir: {e}")))?;

    let ext = if download_url.contains(".tar.gz") {
        "tar.gz"
    } else {
        "zip"
    };
    let temp_file = dir.join(format!("zero-download.{}", ext));

    let client = reqwest::blocking::Client::builder()
        .user_agent("znet-sink")
        .build()
        .map_err(|e| AppError::internal(format!("failed to create http client: {e}")))?;

    let mut response = client
        .get(&download_url)
        .send()
        .map_err(|e| AppError::internal(format!("failed to start download: {e}")))?;

    let bytes_total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let mut hasher = Sha256::new();
    let mut all_bytes = Vec::new();
    let mut bytes_downloaded: u64 = 0;
    let mut last_progress_at: u64 = 0;
    let mut chunk = vec![0u8; CHUNK_SIZE];

    loop {
        let n = response
            .read(&mut chunk)
            .map_err(|e| AppError::internal(format!("failed to read download chunk: {e}")))?;
        if n == 0 {
            break;
        }

        hasher.update(&chunk[..n]);
        all_bytes.extend_from_slice(&chunk[..n]);
        bytes_downloaded += n as u64;

        if bytes_downloaded - last_progress_at >= PROGRESS_INTERVAL || n < CHUNK_SIZE {
            last_progress_at = bytes_downloaded;
            let percent = bytes_total.map(|total| {
                if total > 0 {
                    (bytes_downloaded as f64 / total as f64) * 100.0
                } else {
                    0.0
                }
            });
            let _ = app.emit(
                PROGRESS_EVENT,
                KernelDownloadProgress {
                    version: version.clone(),
                    bytes_downloaded,
                    bytes_total,
                    percent,
                },
            );
        }
    }

    // Final progress event at 100%
    let _ = app.emit(
        PROGRESS_EVENT,
        KernelDownloadProgress {
            version: version.clone(),
            bytes_downloaded,
            bytes_total: Some(bytes_downloaded),
            percent: Some(100.0),
        },
    );

    // Checksum verification
    let hash_hex = format!("{:x}", hasher.finalize());
    let checksum_verified = if let Some(expected) = &expected_sha256 {
        if !hash_hex.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&temp_file);
            return Err(AppError::internal(format!(
                "SHA256 mismatch: expected {}, got {}",
                expected, hash_hex
            )));
        }
        true
    } else {
        false
    };

    // Write temp file
    fs::write(&temp_file, &all_bytes)
        .map_err(|e| AppError::internal(format!("failed to write download: {e}")))?;

    // Extract
    extract_archive(&temp_file, &dir)?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_file);

    let executable_name = if cfg!(windows) { "zero.exe" } else { "zero" };
    let executable_path = dir.join(executable_name);
    if !executable_path.is_file() {
        return Err(AppError::internal(format!(
            "extracted but could not find binary at: {}",
            executable_path.display()
        )));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&executable_path)
            .map_err(|e| AppError::internal(format!("failed to read permissions: {e}")))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&executable_path, perms)
            .map_err(|e| AppError::internal(format!("failed to set executable permissions: {e}")))?;
    }

    let channel = classify_channel(&version, false);

    Ok(KernelInstallResult {
        success: true,
        executable_path: path_to_string(&executable_path),
        version: version.clone(),
        channel,
        checksum_verified,
        message: format!(
            "zero {} installed to {}",
            version,
            path_to_string(&dir)
        ),
    })
}

pub fn detect_installed_version(config: &AppCoreConfig) -> AppResult<KernelVersionDetect> {
    let executable_path = core_config::resolve_executable_path(config);

    match executable_path {
        Some(path) if path.is_file() => {
            let output = Command::new(&path)
                .arg("--version")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let version = extract_semver(&stdout);
                    Ok(KernelVersionDetect {
                        version,
                        source: "cli".to_string(),
                    })
                }
                _ => Ok(KernelVersionDetect {
                    version: None,
                    source: "none".to_string(),
                }),
            }
        }
        _ => Ok(KernelVersionDetect {
            version: None,
            source: "none".to_string(),
        }),
    }
}

/// Extract a semver version from arbitrary `--version` output.
///
/// Handles formats like:
///   `v0.0.5`
///   `zero v0.0.5 (abcdef12 2026-06-02)`
///   `zero 0.0.5\nBuild: abc1234\nTarget: x86_64`
///
/// Returns the version **without** a leading `v`.
fn extract_semver(raw: &str) -> Option<String> {
    for token in raw.split_whitespace() {
        let candidate = token.trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c == ';');
        let version_part = candidate.strip_prefix('v').unwrap_or(candidate);
        if version_part.starts_with(|c: char| c.is_ascii_digit())
            && version_part.chars().filter(|&c| c == '.').count() >= 2
        {
            let semver: String = version_part
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !semver.is_empty() {
                return Some(semver);
            }
        }
    }
    None
}

fn parse_release(
    client: &reqwest::blocking::Client,
    release: &serde_json::Value,
    platform_asset: &'static str,
) -> Option<KernelRelease> {
    let tag = release["tag_name"].as_str()?;
    let version = tag.strip_prefix('v').unwrap_or(tag).to_string();
    let prerelease = release["prerelease"].as_bool().unwrap_or(false);
    let channel = classify_channel(tag, prerelease);

    let published_at_unix_ms = release["published_at"]
        .as_str()
        .and_then(|s| parse_iso8601_to_unix_ms(s));

    let assets = release["assets"].as_array()?;

    let platform = assets.iter().find(|a| {
        a["name"]
            .as_str()
            .map(|n| n == platform_asset)
            .unwrap_or(false)
    })?;

    let asset_download_url = platform["browser_download_url"]
        .as_str()
        .map(|s| s.to_string());

    let asset_size_bytes = platform["size"].as_u64();

    let release_notes_url = release["html_url"].as_str().map(|s| s.to_string());

    let checksum_sha256 = fetch_checksums(client, assets, platform_asset);

    Some(KernelRelease {
        version,
        channel,
        prerelease,
        published_at_unix_ms,
        asset_size_bytes,
        asset_download_url,
        release_notes_url,
        checksum_sha256,
    })
}

fn classify_channel(tag: &str, prerelease: bool) -> ReleaseChannel {
    let tag_lower = tag.to_ascii_lowercase();
    if tag_lower.contains("nightly") || tag_lower.contains("dev") || tag_lower.contains("canary") {
        ReleaseChannel::Nightly
    } else if prerelease {
        ReleaseChannel::Beta
    } else {
        ReleaseChannel::Stable
    }
}

fn platform_asset_name() -> &'static str {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "zero-darwin-aarch64.tar.gz"
    } else if cfg!(target_os = "macos") {
        "zero-darwin-x86_64.tar.gz"
    } else if cfg!(target_os = "linux") {
        "zero-linux-x86_64.tar.gz"
    } else if cfg!(target_os = "windows") {
        "zero-windows-x86_64.zip"
    } else {
        "unknown"
    }
}

fn fetch_checksums(
    client: &reqwest::blocking::Client,
    assets: &[serde_json::Value],
    platform_asset: &str,
) -> Option<String> {
    let checksums_url = assets.iter().find(|a| {
        a["name"]
            .as_str()
            .map(|n| n == "checksums.txt")
            .unwrap_or(false)
    })?["browser_download_url"]
        .as_str()?;

    let mut resp = client.get(checksums_url).send().ok()?;
    let mut body = String::new();
    resp.read_to_string(&mut body).ok()?;

    let reader = std::io::BufReader::new(body.as_bytes());
    for line in reader.lines() {
        if let Ok(line) = line {
            let line = line.trim();
            if line.contains(platform_asset) {
                let hash = line.split_whitespace().next()?;
                return Some(hash.to_string());
            }
        }
    }
    None
}

fn resolve_install_dir(install_dir: Option<String>) -> AppResult<PathBuf> {
    match install_dir {
        Some(d) if !d.trim().is_empty() => Ok(PathBuf::from(d.trim())),
        _ => data_dir().map(|dir| dir.join("core")),
    }
}


fn extract_archive(archive: &Path, dest: &Path) -> AppResult<()> {
    let archive_str = path_to_string(archive);
    let dest_str = path_to_string(dest);

    let status = if archive_str.ends_with(".tar.gz") {
        Command::new("tar")
            .args(["-xzf", &archive_str, "-C", &dest_str])
            .status()
    } else {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    archive_str, dest_str
                ),
            ])
            .status()
    }
    .map_err(|e| AppError::internal(format!("failed to extract: {e}")))?;

    if !status.success() {
        let _ = fs::remove_file(archive);
        return Err(AppError::internal("failed to extract archive"));
    }
    Ok(())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn parse_iso8601_to_unix_ms(s: &str) -> Option<u64> {
    // GitHub returns ISO 8601 like "2026-05-20T10:30:00Z"
    let s = s.trim_end_matches('Z');
    let parts: Vec<&str> = s.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<u32> = parts[0]
        .split('-')
        .filter_map(|p| p.parse().ok())
        .collect();
    if date_parts.len() != 3 {
        return None;
    }

    let time_parts: Vec<u32> = parts[1]
        .split(':')
        .filter_map(|p| p.parse().ok())
        .collect();
    if time_parts.len() < 2 {
        return None;
    }

    // Simplified: use chrono would be better but this avoids adding another dep
    // Approximate unix timestamp
    let year = date_parts[0] as i64;
    let month = date_parts[1] as i64;
    let day = date_parts[2] as i64;
    let hour = time_parts.get(0).copied().unwrap_or(0) as i64;
    let minute = time_parts.get(1).copied().unwrap_or(0) as i64;
    let second = time_parts.get(2).copied().unwrap_or(0) as i64;

    // Days from year 0 approximation
    let days = (year * 365 + year / 4 - year / 100 + year / 400)
        + (month * 30 + month / 2).min(month * 31 - (month + 1) / 2)
        + day;
    let secs = days * 86400 + hour * 3600 + minute * 60 + second;

    // Offset from 1970-01-01 (approximate: 719528 days)
    let unix_secs = secs - 719528 * 86400;
    Some(unix_secs as u64 * 1000)
}
