use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppCoreConfig;
use crate::models::core_process::CoreProcessState;
use crate::models::kernel_version::{
    KernelDownloadProgress, KernelInstallResult, KernelRelease, KernelVersionDetect,
    KernelVersionList, ReleaseChannel,
};
use crate::models::logs::{LogLevel, LogSource};
use crate::services::{common, core_config, core_process, system_proxy_guard};

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/zerodenet/core/releases?per_page=30";
const PROGRESS_EVENT: &str = "kernel:download-progress";
const CHUNK_SIZE: usize = 8 * 1024;
const PROGRESS_INTERVAL: u64 = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600); // 10 min for large archives
const RUNTIME_MANIFEST_FILE: &str = ".znet-sink-zero-runtime.json";

pub struct KernelInstallOutcome {
    pub result: KernelInstallResult,
    pub restart_core: bool,
    pub restore_system_proxy: bool,
}

struct KernelInstallWorkspace {
    root: PathBuf,
}

impl KernelInstallWorkspace {
    fn create() -> AppResult<Self> {
        let parent = data_dir()?.join("kernel-install");
        fs::create_dir_all(&parent).map_err(|e| {
            AppError::internal(format!("failed to create kernel install workspace: {e}"))
        })?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| {
                AppError::internal(format!("failed to create kernel install workspace id: {e}"))
            })?
            .as_nanos();
        let root = parent.join(format!("{}-{nonce}", std::process::id()));
        fs::create_dir(&root).map_err(|e| {
            AppError::internal(format!("failed to create kernel install workspace: {e}"))
        })?;
        Ok(Self { root })
    }
}

impl Drop for KernelInstallWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Build a blocking HTTP client for management traffic.
///
/// Reqwest's default proxy policy follows `HTTPS_PROXY`, `HTTP_PROXY`,
/// `ALL_PROXY`, and `NO_PROXY` from the process environment. The application
/// does not synthesize a proxy from kernel or operating-system state.
fn build_http_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("znet-sink")
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| AppError::internal(format!("failed to create http client: {e}")))
}

pub fn list_available_versions() -> AppResult<KernelVersionList> {
    let client = build_http_client()?;

    let mut resp = client
        .get(GITHUB_RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| AppError::internal(format!("failed to fetch releases: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::internal(format!(
            "failed to fetch releases: HTTP {}",
            resp.status()
        )));
    }

    let mut body = String::new();
    resp.read_to_string(&mut body)
        .map_err(|e| AppError::internal(format!("failed to read releases response: {e}")))?;

    let releases_json: Vec<serde_json::Value> = serde_json::from_str(&body)
        .map_err(|e| AppError::internal(format!("failed to parse releases: {e}")))?;

    let platform_asset = platform_asset_name();
    let mut versions: Vec<KernelRelease> = releases_json
        .into_iter()
        .filter_map(|release| parse_release(&release, platform_asset))
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
) -> AppResult<KernelInstallOutcome> {
    let dir = resolve_install_dir(install_dir)?;
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::internal(format!("failed to create install dir: {e}")))?;
    let workspace = KernelInstallWorkspace::create()?;

    // Log install attempt so user can trace in LogPanel
    {
        let state = app.state::<crate::state::app_state::AppState>();
        let _ = crate::services::logs::append_entry(
            &state,
            LogSource::App,
            LogLevel::Info,
            format!("kernel install: v{version} → {}", dir.display()),
            None,
        );
    }

    let ext = if download_url.contains(".tar.gz") {
        "tar.gz"
    } else {
        "zip"
    };
    let temp_file = workspace.root.join(format!("zero-download.{ext}"));

    let client = build_http_client()?;
    // Version listing is metadata-only. Resolve the checksum lazily for the
    // single release the user actually installs instead of issuing one extra
    // network request per release while opening the version manager.
    let expected_sha256 = match expected_sha256
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value),
        None => fetch_checksum_for_download(&client, &download_url, platform_asset_name())?,
    };

    let _ = crate::services::logs::append_entry(
        &app.state::<crate::state::app_state::AppState>(),
        LogSource::App,
        LogLevel::Info,
        format!("kernel download: GET {download_url}"),
        None,
    );

    let _ = app.emit(
        PROGRESS_EVENT,
        KernelDownloadProgress {
            version: version.clone(),
            bytes_downloaded: 0,
            bytes_total: None,
            percent: None,
        },
    );

    let mut response = client.get(&download_url).send().map_err(|e| {
        let msg = format!(
            "内核下载失败: {e}（网络访问遵循应用进程环境；需要代理时请检查 HTTPS_PROXY / HTTP_PROXY / ALL_PROXY）"
        );
        let _ = crate::services::logs::append_entry(
            &app.state::<crate::state::app_state::AppState>(),
            LogSource::App,
            LogLevel::Error,
            msg.clone(),
            None,
        );
        AppError::internal(msg)
    })?;

    if !response.status().is_success() {
        let msg = format!(
            "内核下载失败: HTTP {}（请检查版本资产和网络访问权限）",
            response.status()
        );
        let _ = crate::services::logs::append_entry(
            &app.state::<crate::state::app_state::AppState>(),
            LogSource::App,
            LogLevel::Error,
            msg.clone(),
            None,
        );
        return Err(AppError::internal(msg));
    }

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
            return Err(AppError::internal(format!(
                "SHA256 mismatch: expected {}, got {}",
                expected, hash_hex
            )));
        }
        true
    } else {
        false
    };

    // Write the downloaded archive only inside ZNet-Sink's private install
    // workspace. User-selected kernel directories never receive temporary
    // files such as zero-download.zip or .staging.
    fs::write(&temp_file, &all_bytes)
        .map_err(|e| AppError::internal(format!("failed to write download: {e}")))?;

    let staging = workspace.root.join("staging");
    fs::create_dir(&staging)
        .map_err(|e| AppError::internal(format!("failed to create staging dir: {e}")))?;
    extract_archive(&temp_file, &staging)?;

    let executable_name = if cfg!(windows) { "zero.exe" } else { "zero" };

    // The archive may contain the binary directly or nested inside a
    // subdirectory (e.g. zero-windows-x86_64/zero.exe).  Search for it.
    let staged_binary = find_file_recursive(&staging, executable_name).ok_or_else(|| {
        AppError::internal(format!(
            "extracted but could not find '{}' in staging directory",
            executable_name
        ))
    })?;
    let staged_bundle_dir = staged_binary
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| staging.clone());

    // Target path in the install directory.
    let executable_path = dir.join(executable_name);
    let state = app.state::<crate::state::app_state::AppState>();
    let current_executable_path = {
        let app_config = common::lock(state.app_config(), "app_config")?;
        core_config::resolve_executable_path(&app_config.core)
    };
    let default_install_dir = data_dir()?.join("core");
    let managed_default_dir = same_path(&dir, &default_install_dir);
    let target_is_current_core = current_executable_path
        .as_deref()
        .is_some_and(|path| same_path(path, &executable_path));
    let previous_managed_files = read_runtime_manifest(&dir)?;
    let bundle_files = collect_runtime_bundle_files(&staged_bundle_dir, executable_name)?;

    // Validate every same-name target before the currently working core is
    // stopped. A dedicated ZNet-Sink target can replace its legacy binary and
    // known Wintun companion; all other existing files must either be tracked
    // by our manifest or byte-identical to the official bundle entry.
    validate_runtime_bundle_targets(
        &staged_bundle_dir,
        &dir,
        executable_name,
        &bundle_files,
        &previous_managed_files,
        managed_default_dir,
        target_is_current_core,
    )?;

    let restart_core =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let restore_system_proxy =
        restart_core && system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
    let _ = crate::services::logs::append_entry(
        state.inner(),
        LogSource::App,
        LogLevel::Info,
        format!("kernel upgrade: stopping core before swapping in v{version}"),
        None,
    );

    // Keep the kernel running during the network transfer so environments
    // that depend on the kernel's mixed-port can still reach the release
    // asset. We only stop it immediately before replacing the executable.
    // This is a short in-place replacement and the restarted kernel keeps the
    // same local endpoint. Preserve the guarded OS proxy so macOS does not ask
    // for authorization once to disable it and again to re-enable it.
    core_process::stop_preserving_system_proxy(app.clone(), state.clone())?;

    let _ = crate::services::logs::append_entry(
        state.inner(),
        LogSource::App,
        LogLevel::Info,
        format!(
            "kernel upgrade: replacing binary at {}",
            executable_path.display()
        ),
        None,
    );

    // Remove old binary.  The kernel was killed before we got here but
    // Windows may hold the file handle briefly.  Retry a few times.
    if executable_path.exists() {
        remove_file_with_retry(&executable_path, 5)?;
    }

    // Move the new binary into place
    if fs::rename(&staged_binary, &executable_path).is_err() {
        // Cross-device or other rename failure — fall back to copy
        fs::copy(&staged_binary, &executable_path).map_err(|e| {
            AppError::internal(format!("failed to copy binary to install dir: {e}"))
        })?;
    }

    // Zero release archives are runtime bundles, not just executable
    // containers. Preserve any files shipped adjacent to the binary (for
    // example Windows `wintun.dll`) instead of deleting them with staging.
    // The core release remains the authority for which companions exist and
    // where they come from; ZNet-Sink only preserves the published bundle.
    let companions =
        install_runtime_companions(&staged_bundle_dir, &dir, executable_name, &bundle_files)?;
    if !companions.is_empty() {
        let _ = crate::services::logs::append_entry(
            state.inner(),
            LogSource::App,
            LogLevel::Info,
            format!(
                "kernel install: preserved runtime companions: {}",
                companions.join(", ")
            ),
            None,
        );
    }

    // Keep ownership records across downgrade/legacy bundles that may omit a
    // previously installed companion. We do not delete absent companions
    // implicitly; doing so could make an older Zero build unusable when its
    // historical release archive omitted a required runtime file.
    let mut next_managed_files = previous_managed_files;
    next_managed_files.extend(bundle_files.iter().cloned());
    write_runtime_manifest(&dir, &next_managed_files)?;

    if !executable_path.is_file() {
        return Err(AppError::internal(format!(
            "binary missing after install: {}",
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
        fs::set_permissions(&executable_path, perms).map_err(|e| {
            AppError::internal(format!("failed to set executable permissions: {e}"))
        })?;
    }

    let channel = classify_channel(&version, false);

    let _ = crate::services::logs::append_entry(
        state.inner(),
        LogSource::App,
        LogLevel::Info,
        format!(
            "kernel install complete: v{version} → {}",
            executable_path.display()
        ),
        None,
    );

    Ok(KernelInstallOutcome {
        result: KernelInstallResult {
            success: true,
            executable_path: path_to_string(&executable_path),
            version: version.clone(),
            channel,
            checksum_verified,
            message: format!("zero {} installed to {}", version, path_to_string(&dir)),
        },
        restart_core,
        restore_system_proxy,
    })
}

pub fn detect_installed_version(config: &AppCoreConfig) -> AppResult<KernelVersionDetect> {
    let executable_path = core_config::resolve_executable_path(config);

    match executable_path {
        Some(path) if path.is_file() => {
            let path_string = path_to_string(&path);
            let version = detect_cli_version(&path);
            Ok(KernelVersionDetect {
                source: if version.is_some() { "cli" } else { "file" }.to_string(),
                version,
                executable_path: Some(path_string),
                executable_exists: true,
            })
        }
        Some(path) => Ok(KernelVersionDetect {
            version: None,
            source: "missing".to_string(),
            executable_path: Some(path_to_string(&path)),
            executable_exists: false,
        }),
        None => Ok(KernelVersionDetect {
            version: None,
            source: "none".to_string(),
            executable_path: None,
            executable_exists: false,
        }),
    }
}

fn detect_cli_version(path: &Path) -> Option<String> {
    let program = path.to_str()?;
    for args in [["--version"].as_slice(), ["version"].as_slice()] {
        let output = common::background_command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if let Some(version) = extract_semver(&stdout) {
                return Some(version);
            }
            if let Some(version) = extract_semver(&stderr) {
                return Some(version);
            }
        }
    }

    None
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
        let candidate = token.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '"' | '\''
            )
        });
        let version_part = candidate.strip_prefix('v').unwrap_or(candidate);
        // A real semver parser preserves pre-release/build metadata (for
        // example `0.0.15-bate.1`) and naturally rejects IPv4 addresses.
        if let Ok(version) = semver::Version::parse(version_part) {
            return Some(version.to_string());
        }
    }
    None
}

fn parse_release(
    release: &serde_json::Value,
    platform_asset: &'static str,
) -> Option<KernelRelease> {
    let tag = release["tag_name"].as_str()?;
    let version = tag.strip_prefix('v').unwrap_or(tag).to_string();
    let prerelease = release["prerelease"].as_bool().unwrap_or(false);
    let channel = classify_channel(tag, prerelease);

    let published_at_unix_ms = release["published_at"]
        .as_str()
        .and_then(parse_iso8601_to_unix_ms);

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

    Some(KernelRelease {
        version,
        channel,
        prerelease,
        published_at_unix_ms,
        asset_size_bytes,
        asset_download_url,
        release_notes_url,
        // Keep the wire shape compatible, but do not fetch checksums while
        // listing releases. install_version resolves this lazily instead.
        checksum_sha256: None,
    })
}

/// Classify a release tag into a channel.
///
/// The kernel's own versioning strategy (docs.zerodenet.org) is:
/// - `x-beta` → internal test (Beta)
/// - `x-rc`   → pre-release candidate (Beta)
/// - no suffix → stable release (Stable)
///
/// We also respect GitHub's `prerelease` flag and explicit nightly/dev/canary
/// keywords so that mislabeled releases are still routed correctly.
fn classify_channel(tag: &str, prerelease: bool) -> ReleaseChannel {
    let tag_lower = tag.to_ascii_lowercase();
    if tag_lower.contains("nightly") || tag_lower.contains("dev") || tag_lower.contains("canary") {
        ReleaseChannel::Nightly
    } else if tag_lower.contains("-beta") || tag_lower.contains("-rc") || prerelease {
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

fn fetch_checksum_for_download(
    client: &reqwest::blocking::Client,
    download_url: &str,
    platform_asset: &str,
) -> AppResult<Option<String>> {
    let Some((release_root, _)) = download_url.rsplit_once('/') else {
        return Ok(None);
    };
    // Current releases publish one checksum sidecar per asset. Keep the
    // aggregate manifest as a fallback for older releases.
    let checksum_urls = [
        format!("{download_url}.sha256"),
        format!("{release_root}/checksums.txt"),
    ];
    for checksum_url in checksum_urls {
        let Ok(mut response) = client.get(&checksum_url).send() else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let mut body = String::new();
        if response.read_to_string(&mut body).is_err() {
            continue;
        }
        if let Some(hash) = parse_checksum(&body, platform_asset) {
            return Ok(Some(hash));
        }
    }
    Ok(None)
}

fn parse_checksum(body: &str, platform_asset: &str) -> Option<String> {
    for line in body.lines() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let file_name = fields
            .next()
            .map(|value| value.trim_start_matches('*'))
            .unwrap_or("");
        let valid_hash = hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit());
        if valid_hash && (file_name.is_empty() || file_name == platform_asset) {
            return Some(hash.to_ascii_lowercase());
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
        common::background_command("tar")
            .args(["-xzf", &archive_str, "-C", &dest_str])
            .status()
    } else {
        common::background_command("powershell")
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
        return Err(AppError::internal("failed to extract archive"));
    }
    Ok(())
}

/// Recursively search for a file named `name` inside `dir`.
fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

fn collect_runtime_bundle_files(
    bundle_dir: &Path,
    executable_name: &str,
) -> AppResult<Vec<String>> {
    let entries = fs::read_dir(bundle_dir)
        .map_err(|e| AppError::internal(format!("failed to read kernel runtime bundle: {e}")))?;
    let mut files = BTreeSet::new();

    for entry in entries {
        let entry = entry.map_err(|e| {
            AppError::internal(format!("failed to inspect kernel runtime bundle: {e}"))
        })?;
        let file_type = entry.file_type().map_err(|e| {
            AppError::internal(format!("failed to inspect kernel runtime file: {e}"))
        })?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry
            .file_name()
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| {
                AppError::internal("kernel runtime bundle contains a non-UTF-8 file name")
            })?;
        if file_name == RUNTIME_MANIFEST_FILE {
            return Err(AppError::internal(format!(
                "kernel runtime bundle uses reserved file name '{}'",
                RUNTIME_MANIFEST_FILE
            )));
        }
        files.insert(file_name);
    }

    if !files.contains(executable_name) {
        return Err(AppError::internal(format!(
            "kernel runtime bundle directory does not contain '{}'",
            executable_name
        )));
    }

    Ok(files.into_iter().collect())
}

fn read_runtime_manifest(install_dir: &Path) -> AppResult<BTreeSet<String>> {
    let path = install_dir.join(RUNTIME_MANIFEST_FILE);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let metadata = fs::symlink_metadata(&path).map_err(|e| {
        AppError::internal(format!("failed to inspect kernel runtime manifest: {e}"))
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(AppError::internal(format!(
            "kernel runtime manifest path is not a regular file: {}",
            path.display()
        )));
    }

    let raw = fs::read_to_string(&path)
        .map_err(|e| AppError::internal(format!("failed to read kernel runtime manifest: {e}")))?;
    let files: Vec<String> = serde_json::from_str(&raw)
        .map_err(|e| AppError::internal(format!("invalid kernel runtime manifest: {e}")))?;
    let mut result = BTreeSet::new();
    for file in files {
        if !safe_runtime_file_name(&file) || file == RUNTIME_MANIFEST_FILE {
            return Err(AppError::internal(format!(
                "invalid file name in kernel runtime manifest: {file}"
            )));
        }
        result.insert(file);
    }
    Ok(result)
}

fn write_runtime_manifest(install_dir: &Path, files: &BTreeSet<String>) -> AppResult<()> {
    let path = install_dir.join(RUNTIME_MANIFEST_FILE);
    let payload = serde_json::to_vec_pretty(&files.iter().collect::<Vec<_>>()).map_err(|e| {
        AppError::internal(format!("failed to serialize kernel runtime manifest: {e}"))
    })?;
    fs::write(&path, payload)
        .map_err(|e| AppError::internal(format!("failed to write kernel runtime manifest: {e}")))?;
    Ok(())
}

fn validate_runtime_bundle_targets(
    bundle_dir: &Path,
    install_dir: &Path,
    executable_name: &str,
    bundle_files: &[String],
    previous_managed_files: &BTreeSet<String>,
    managed_default_dir: bool,
    target_is_current_core: bool,
) -> AppResult<()> {
    for file_name in bundle_files {
        let target = install_dir.join(file_name);
        if !target.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&target).map_err(|e| {
            AppError::internal(format!("failed to inspect '{}': {e}", target.display()))
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(runtime_file_conflict(&target));
        }

        let source = bundle_dir.join(file_name);
        if files_are_identical(&source, &target)? {
            continue;
        }

        let tracked = previous_managed_files.contains(file_name);
        let legacy_binary =
            file_name == executable_name && (managed_default_dir || target_is_current_core);
        let legacy_companion = known_legacy_runtime_companion(file_name)
            && (managed_default_dir || target_is_current_core);
        if tracked || legacy_binary || legacy_companion {
            continue;
        }

        return Err(runtime_file_conflict(&target));
    }
    Ok(())
}

fn runtime_file_conflict(target: &Path) -> AppError {
    AppError::invalid_argument(format!(
        "kernel install conflict: '{}' already exists and is not managed by ZNet-Sink; choose a dedicated install directory or rename/remove the conflicting file",
        target.display()
    ))
}

fn known_legacy_runtime_companion(file_name: &str) -> bool {
    cfg!(windows) && file_name.eq_ignore_ascii_case("wintun.dll")
}

fn safe_runtime_file_name(file_name: &str) -> bool {
    if file_name.is_empty() || file_name == "." || file_name == ".." {
        return false;
    }
    !file_name.contains('/') && !file_name.contains('\\')
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());

    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn files_are_identical(left: &Path, right: &Path) -> AppResult<bool> {
    let left_meta = fs::metadata(left)
        .map_err(|e| AppError::internal(format!("failed to inspect '{}': {e}", left.display())))?;
    let right_meta = fs::metadata(right)
        .map_err(|e| AppError::internal(format!("failed to inspect '{}': {e}", right.display())))?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }

    let left_bytes = fs::read(left)
        .map_err(|e| AppError::internal(format!("failed to read '{}': {e}", left.display())))?;
    let right_bytes = fs::read(right)
        .map_err(|e| AppError::internal(format!("failed to read '{}': {e}", right.display())))?;
    Ok(left_bytes == right_bytes)
}

/// Copy files published next to the Zero binary into the managed install
/// directory. The release archive is authoritative for runtime companions;
/// ZNet-Sink does not independently download or synthesize them.
fn install_runtime_companions(
    bundle_dir: &Path,
    install_dir: &Path,
    executable_name: &str,
    bundle_files: &[String],
) -> AppResult<Vec<String>> {
    let mut installed = Vec::new();

    for file_name in bundle_files {
        if file_name == executable_name {
            continue;
        }
        let source = bundle_dir.join(file_name);
        let target = install_dir.join(file_name);

        if target.exists() && files_are_identical(&source, &target)? {
            installed.push(file_name.clone());
            continue;
        }
        if target.exists() {
            remove_file_with_retry(&target, 5)?;
        }
        fs::copy(&source, &target).map_err(|e| {
            AppError::internal(format!(
                "failed to install kernel runtime companion '{}' to '{}': {e}",
                source.display(),
                target.display()
            ))
        })?;
        installed.push(file_name.clone());
    }

    installed.sort();
    Ok(installed)
}

/// Try to remove a file, retrying with short sleeps between attempts.
/// On Windows the OS may briefly hold a file handle after the process
/// that used it has been killed.
fn remove_file_with_retry(path: &Path, max_attempts: u32) -> AppResult<()> {
    for attempt in 0..max_attempts {
        if fs::remove_file(path).is_ok() {
            return Ok(());
        }
        if attempt + 1 < max_attempts {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    Err(AppError::internal(format!(
        "failed to remove '{}' after {} attempts — is the kernel still running?",
        path.display(),
        max_attempts
    )))
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

    let date_parts: Vec<i64> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() != 3 {
        return None;
    }

    let time_parts: Vec<i64> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
    if time_parts.len() < 2 {
        return None;
    }

    let year = date_parts[0];
    let month = date_parts[1];
    let day = date_parts[2];
    let hour = time_parts.first().copied().unwrap_or(0);
    let minute = time_parts.get(1).copied().unwrap_or(0);
    let second = time_parts.get(2).copied().unwrap_or(0);

    // days_from_civil handles leap years/centuries correctly.  The old
    // month-day approximation counted the current month as elapsed days,
    // inflating the result by ~30 days and making every release appear
    // one month in the future.
    let days = days_from_civil(year, month, day);
    let secs = days * 86400 + hour * 3600 + minute * 60 + second;
    if secs < 0 {
        return None;
    }
    Some(secs as u64 * 1000)
}

/// Days since 1970-01-01 for a proleptic-Gregorian (year, month, day).
/// Howard Hinnant's algorithm — valid for any date, handles all leap-year
/// rules without depending on chrono.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_runtime_dirs(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let unique = format!(
            "znet-kernel-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let bundle = root.join("bundle");
        let install = root.join("install");
        fs::create_dir_all(&bundle).unwrap();
        fs::create_dir_all(&install).unwrap();
        (root, bundle, install)
    }

    #[test]
    fn extracts_stable_and_prefixed_semver() {
        assert_eq!(extract_semver("zero v0.0.15"), Some("0.0.15".to_string()));
        assert_eq!(extract_semver("build_id: 1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn preserves_prerelease_and_build_metadata() {
        assert_eq!(
            extract_semver("build_id: 0.0.15-bate.1\ngit: v0.0.15-bate.1"),
            Some("0.0.15-bate.1".to_string())
        );
        assert_eq!(
            extract_semver("zero v1.2.3-rc.2+windows.x86-64"),
            Some("1.2.3-rc.2+windows.x86-64".to_string())
        );
    }

    #[test]
    fn rejects_network_addresses_as_versions() {
        assert_eq!(extract_semver("listening on 127.0.0.1:8080"), None);
        assert_eq!(extract_semver("control 0.0.0.0"), None);
    }

    #[test]
    fn parses_checksum_for_exact_platform_asset() {
        let hash = "a".repeat(64);
        let other_hash = "b".repeat(64);
        let body =
            format!("{other_hash}  zero-linux-x86_64.tar.gz\n{hash} *zero-darwin-aarch64.tar.gz\n");
        assert_eq!(
            parse_checksum(&body, "zero-darwin-aarch64.tar.gz"),
            Some(hash)
        );
    }

    #[test]
    fn parses_checksum_sidecar_with_asset_name() {
        let hash = "c".repeat(64);
        let body = format!("{hash}  zero-darwin-x86_64.tar.gz\n");
        assert_eq!(
            parse_checksum(&body, "zero-darwin-x86_64.tar.gz"),
            Some(hash)
        );
    }

    #[test]
    fn preserves_runtime_companions_from_the_binary_directory() {
        let (root, bundle, install) = temp_runtime_dirs("bundle");
        fs::write(bundle.join("zero.exe"), b"exe").unwrap();
        fs::write(bundle.join("wintun.dll"), b"dll").unwrap();
        fs::write(bundle.join("NOTICE.txt"), b"notice").unwrap();

        let bundle_files = collect_runtime_bundle_files(&bundle, "zero.exe").unwrap();
        let installed =
            install_runtime_companions(&bundle, &install, "zero.exe", &bundle_files).unwrap();

        assert_eq!(
            installed,
            vec!["NOTICE.txt".to_string(), "wintun.dll".to_string()]
        );
        assert_eq!(fs::read(install.join("wintun.dll")).unwrap(), b"dll");
        assert_eq!(fs::read(install.join("NOTICE.txt")).unwrap(), b"notice");
        assert!(!install.join("zero.exe").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unmanaged_same_name_runtime_conflicts_before_install() {
        let (root, bundle, install) = temp_runtime_dirs("conflict");
        fs::write(bundle.join("zero.exe"), b"new-exe").unwrap();
        fs::write(bundle.join("NOTICE.txt"), b"official").unwrap();
        fs::write(install.join("NOTICE.txt"), b"user-file").unwrap();

        let bundle_files = collect_runtime_bundle_files(&bundle, "zero.exe").unwrap();
        let error = validate_runtime_bundle_targets(
            &bundle,
            &install,
            "zero.exe",
            &bundle_files,
            &BTreeSet::new(),
            false,
            false,
        )
        .unwrap_err();

        assert!(error.message.contains("kernel install conflict"));
        assert_eq!(fs::read(install.join("NOTICE.txt")).unwrap(), b"user-file");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tracked_runtime_companion_can_be_replaced() {
        let (root, bundle, install) = temp_runtime_dirs("tracked");
        fs::write(bundle.join("zero.exe"), b"new-exe").unwrap();
        fs::write(bundle.join("runtime.dat"), b"new-runtime").unwrap();
        fs::write(install.join("runtime.dat"), b"old-runtime").unwrap();
        let mut tracked = BTreeSet::new();
        tracked.insert("runtime.dat".to_string());

        let bundle_files = collect_runtime_bundle_files(&bundle, "zero.exe").unwrap();
        validate_runtime_bundle_targets(
            &bundle,
            &install,
            "zero.exe",
            &bundle_files,
            &tracked,
            false,
            false,
        )
        .unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identical_unmanaged_runtime_file_is_safe_to_adopt() {
        let (root, bundle, install) = temp_runtime_dirs("identical");
        fs::write(bundle.join("zero.exe"), b"new-exe").unwrap();
        fs::write(bundle.join("NOTICE.txt"), b"same").unwrap();
        fs::write(install.join("NOTICE.txt"), b"same").unwrap();

        let bundle_files = collect_runtime_bundle_files(&bundle, "zero.exe").unwrap();
        validate_runtime_bundle_targets(
            &bundle,
            &install,
            "zero.exe",
            &bundle_files,
            &BTreeSet::new(),
            false,
            false,
        )
        .unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_manifest_round_trips_managed_names() {
        let (root, _bundle, install) = temp_runtime_dirs("manifest");
        let files = BTreeSet::from([
            "zero.exe".to_string(),
            "wintun.dll".to_string(),
            "NOTICE.txt".to_string(),
        ]);
        write_runtime_manifest(&install, &files).unwrap();
        assert_eq!(read_runtime_manifest(&install).unwrap(), files);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn days_from_civil_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        // 1970 is not a leap year
        assert_eq!(days_from_civil(1971, 1, 1), 365);
    }

    #[test]
    fn days_from_civil_leap_year_rules() {
        // 1972 is a leap year: Mar 1 = 365 + 365 + 31(Jan) + 29(Feb)
        assert_eq!(days_from_civil(1972, 3, 1), 790);
        // 2000 is a leap year (divisible by 400)
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        // 2100 is NOT a leap year (divisible by 100 but not 400)
        assert_eq!(days_from_civil(2100, 3, 1), 47541);
    }

    #[test]
    fn parse_iso8601_known_timestamp() {
        // 2026-05-20T10:30:00Z = 1779273000 seconds
        let ms = parse_iso8601_to_unix_ms("2026-05-20T10:30:00Z").unwrap();
        assert_eq!(ms, 1_779_273_000_000);
    }

    #[test]
    fn parse_iso8601_rejects_malformed() {
        assert_eq!(parse_iso8601_to_unix_ms("not-a-date"), None);
        assert_eq!(parse_iso8601_to_unix_ms("2026-05-20"), None); // missing T-time
    }
}
