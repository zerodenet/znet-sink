//! Windows compatibility for Zero releases that advertise TUN support but
//! predate the self-contained Windows runtime bundle.
//!
//! The compatibility path is intentionally lazy:
//! - it runs only after Zero has reported TUN support;
//! - it only repairs ZNet-Sink-managed runtimes;
//! - it never replaces an existing `wintun.dll`;
//! - the pinned provenance matches Zero's release workflow.

#[cfg(windows)]
use std::collections::BTreeSet;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use sha2::{Digest, Sha256};

use crate::errors::{AppError, AppResult};

#[cfg(windows)]
use crate::services::{app_config_store, common, core_config};

#[cfg(any(windows, test))]
const WINTUN_VERSION: &str = "0.14.1";
#[cfg(any(windows, test))]
const WINTUN_ARCHIVE_SHA256: &str =
    "07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51";
#[cfg(any(windows, test))]
const WINTUN_ARCHIVE_URL: &str = "https://www.wintun.net/builds/wintun-0.14.1.zip";
#[cfg(windows)]
const WINTUN_DLL: &str = "wintun.dll";
#[cfg(windows)]
const WINTUN_LICENSE: &str = "wintun-LICENSE.txt";
#[cfg(windows)]
const RUNTIME_MANIFEST_FILE: &str = ".znet-sink-zero-runtime.json";
#[cfg(windows)]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn ensure_for_current_runtime() -> AppResult<()> {
    #[cfg(not(windows))]
    {
        return Ok(());
    }

    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(ensure_for_current_runtime_blocking)
            .await
            .map_err(|error| {
                AppError::internal(format!("Wintun compatibility task failed: {error}"))
            })?
    }
}

#[cfg(windows)]
fn ensure_for_current_runtime_blocking() -> AppResult<()> {
    let config_path = app_config_store::default_config_path()?;
    let app_config = app_config_store::load_or_default(&config_path)?;
    let executable = core_config::resolve_executable_path(&app_config.core).ok_or_else(|| {
        AppError::invalid_argument(
            "Zero supports TUN, but its executable path cannot be resolved; configure a managed Zero runtime or provide wintun.dll next to the external zero.exe",
        )
    })?;
    let install_dir = executable.parent().ok_or_else(|| {
        AppError::invalid_argument("Zero executable path has no parent runtime directory")
    })?;

    let wintun_target = install_dir.join(WINTUN_DLL);
    if wintun_target.exists() {
        ensure_regular_file(&wintun_target, "Wintun runtime")?;
        return Ok(());
    }

    if !is_managed_runtime(&executable, install_dir)? {
        return Err(AppError::invalid_argument(format!(
            "Zero supports TUN but '{}' is missing next to the external runtime; ZNet-Sink only repairs its managed Zero runtime automatically",
            WINTUN_DLL
        )));
    }

    repair_managed_runtime(&executable, install_dir)
}

#[cfg(windows)]
fn is_managed_runtime(executable: &Path, install_dir: &Path) -> AppResult<bool> {
    let default_executable = crate::services::data_dir()?.join("core").join("zero.exe");
    if same_path(executable, &default_executable) {
        return Ok(true);
    }

    let managed_files = read_runtime_manifest(install_dir)?;
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("zero.exe");
    Ok(managed_files.contains(executable_name))
}

#[cfg(windows)]
fn repair_managed_runtime(executable: &Path, install_dir: &Path) -> AppResult<()> {
    let workspace = WintunWorkspace::create()?;
    let archive_path = workspace.root.join("wintun.zip");
    let extract_dir = workspace.root.join("extracted");

    let archive = download_pinned_archive()?;
    verify_archive_sha256(&archive)?;
    fs::write(&archive_path, archive)
        .map_err(|error| AppError::internal(format!("failed to stage Wintun archive: {error}")))?;
    fs::create_dir(&extract_dir).map_err(|error| {
        AppError::internal(format!(
            "failed to create Wintun extraction directory: {error}"
        ))
    })?;
    expand_archive(&archive_path, &extract_dir)?;

    let distribution_root = extract_dir.join("wintun");
    let dll_source = distribution_root.join("bin").join("amd64").join(WINTUN_DLL);
    let license_source = distribution_root.join("LICENSE.txt");
    ensure_regular_file(&dll_source, "pinned Wintun amd64 runtime")?;
    ensure_regular_file(&license_source, "pinned Wintun license")?;

    let dll_target = install_dir.join(WINTUN_DLL);
    let license_target = install_dir.join(WINTUN_LICENSE);
    if dll_target.exists() {
        ensure_regular_file(&dll_target, "existing Wintun runtime")?;
        return Ok(());
    }

    if license_target.exists() {
        ensure_regular_file(&license_target, "existing Wintun license")?;
        if !files_are_identical(&license_source, &license_target)? {
            return Err(AppError::invalid_argument(format!(
                "cannot repair managed Zero runtime because '{}' already exists with different contents",
                license_target.display()
            )));
        }
    }

    let mut managed_files = read_runtime_manifest(install_dir)?;
    if let Some(executable_name) = executable.file_name().and_then(|name| name.to_str()) {
        managed_files.insert(executable_name.to_string());
    }

    let copied_license = !license_target.exists();
    if copied_license {
        fs::copy(&license_source, &license_target).map_err(|error| {
            AppError::internal(format!(
                "failed to install Wintun license to '{}': {error}",
                license_target.display()
            ))
        })?;
    }

    fs::copy(&dll_source, &dll_target).map_err(|error| {
        if copied_license {
            let _ = fs::remove_file(&license_target);
        }
        AppError::internal(format!(
            "failed to install Wintun {} runtime to '{}': {error}",
            WINTUN_VERSION,
            dll_target.display()
        ))
    })?;

    managed_files.insert(WINTUN_DLL.to_string());
    managed_files.insert(WINTUN_LICENSE.to_string());
    if let Err(error) = write_runtime_manifest(install_dir, &managed_files) {
        let _ = fs::remove_file(&dll_target);
        if copied_license {
            let _ = fs::remove_file(&license_target);
        }
        return Err(error);
    }

    Ok(())
}

#[cfg(windows)]
fn download_pinned_archive() -> AppResult<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("znet-sink")
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| {
            AppError::internal(format!("failed to create Wintun download client: {error}"))
        })?;

    let mut response = client
        .get(WINTUN_ARCHIVE_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            AppError::internal(format!(
                "failed to download pinned Wintun {} runtime: {error}",
                WINTUN_VERSION
            ))
        })?;
    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes).map_err(|error| {
        AppError::internal(format!("failed to read Wintun download response: {error}"))
    })?;
    Ok(bytes)
}

#[cfg(windows)]
fn verify_archive_sha256(bytes: &[u8]) -> AppResult<()> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(WINTUN_ARCHIVE_SHA256) {
        return Err(AppError::internal(format!(
            "Wintun {} archive checksum mismatch: expected {}, got {}",
            WINTUN_VERSION, WINTUN_ARCHIVE_SHA256, actual
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn expand_archive(archive: &Path, destination: &Path) -> AppResult<()> {
    let archive = powershell_literal(archive);
    let destination = powershell_literal(destination);
    let status = common::background_command("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive, destination
            ),
        ])
        .status()
        .map_err(|error| {
            AppError::internal(format!(
                "failed to launch Wintun archive extraction: {error}"
            ))
        })?;
    if !status.success() {
        return Err(AppError::internal(
            "failed to extract pinned Wintun archive",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn powershell_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(windows)]
fn ensure_regular_file(path: &Path, label: &str) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::internal(format!("failed to inspect {label}: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(AppError::invalid_argument(format!(
            "{label} is not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn read_runtime_manifest(install_dir: &Path) -> AppResult<BTreeSet<String>> {
    let path = install_dir.join(RUNTIME_MANIFEST_FILE);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    ensure_regular_file(&path, "Zero runtime ownership manifest")?;
    let raw = fs::read_to_string(&path).map_err(|error| {
        AppError::internal(format!(
            "failed to read Zero runtime ownership manifest: {error}"
        ))
    })?;
    let files: Vec<String> = serde_json::from_str(&raw).map_err(|error| {
        AppError::internal(format!("invalid Zero runtime ownership manifest: {error}"))
    })?;
    Ok(files.into_iter().collect())
}

#[cfg(windows)]
fn write_runtime_manifest(install_dir: &Path, files: &BTreeSet<String>) -> AppResult<()> {
    let path = install_dir.join(RUNTIME_MANIFEST_FILE);
    let payload =
        serde_json::to_vec_pretty(&files.iter().collect::<Vec<_>>()).map_err(|error| {
            AppError::internal(format!(
                "failed to serialize Zero runtime ownership manifest: {error}"
            ))
        })?;
    fs::write(&path, payload).map_err(|error| {
        AppError::internal(format!(
            "failed to write Zero runtime ownership manifest: {error}"
        ))
    })
}

#[cfg(windows)]
fn files_are_identical(left: &Path, right: &Path) -> AppResult<bool> {
    let left_bytes = fs::read(left).map_err(|error| {
        AppError::internal(format!("failed to read '{}': {error}", left.display()))
    })?;
    let right_bytes = fs::read(right).map_err(|error| {
        AppError::internal(format!("failed to read '{}': {error}", right.display()))
    })?;
    Ok(left_bytes == right_bytes)
}

#[cfg(windows)]
fn same_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(windows)]
struct WintunWorkspace {
    root: PathBuf,
}

#[cfg(windows)]
impl WintunWorkspace {
    fn create() -> AppResult<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| {
                AppError::internal(format!("failed to create Wintun workspace id: {error}"))
            })?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("znet-sink-wintun-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).map_err(|error| {
            AppError::internal(format!("failed to create Wintun workspace: {error}"))
        })?;
        Ok(Self { root })
    }
}

#[cfg(windows)]
impl Drop for WintunWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::{WINTUN_ARCHIVE_SHA256, WINTUN_ARCHIVE_URL, WINTUN_VERSION};

    #[test]
    fn pinned_wintun_provenance_matches_zero_release_contract() {
        assert_eq!(WINTUN_VERSION, "0.14.1");
        assert_eq!(
            WINTUN_ARCHIVE_SHA256,
            "07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51"
        );
        assert_eq!(
            WINTUN_ARCHIVE_URL,
            "https://www.wintun.net/builds/wintun-0.14.1.zip"
        );
    }
}
