use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};
use crate::services::common;

pub(super) fn validate(binary: &Path, version: &str, config: Option<&Path>) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).map_err(
            |error| AppError::internal(format!("failed to prepare candidate executable: {error}")),
        )?;
    }
    let output = run(binary, &["--version"], Duration::from_secs(10))?;
    if super::extract_semver(&output).as_deref() != Some(version.trim_start_matches('v')) {
        return Err(AppError::invalid_argument(
            "下载的内核版本与所选版本不一致，当前内核保持不变",
        ));
    }
    let help = run(binary, &["help"], Duration::from_secs(10))?;
    if !help.contains("--parent-lifetime-stdin") {
        return Err(AppError::invalid_argument(
            "所选内核不支持客户端托管生命周期，请选择较新的内核版本；当前内核保持不变",
        ));
    }
    if let Some(path) = config.filter(|path| path.is_file()) {
        let path = path
            .to_str()
            .ok_or_else(|| AppError::invalid_argument("kernel config path is not UTF-8"))?;
        run(binary, &["validate", path], Duration::from_secs(15))?;
    }
    Ok(())
}

/// Bound CLI execution and capture output in a private file so a noisy or
/// hung candidate cannot deadlock on a full pipe or freeze the installer.
pub(super) fn run(binary: &Path, args: &[&str], timeout: Duration) -> AppResult<String> {
    let mut output = tempfile::tempfile().map_err(io_error)?;
    let program = binary
        .to_str()
        .ok_or_else(|| AppError::invalid_argument("kernel path is not UTF-8"))?;
    let mut child = common::background_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(io_error)?))
        .stderr(Stdio::from(output.try_clone().map_err(io_error)?))
        .spawn()
        .map_err(io_error)?;
    let result = (|| {
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(io_error)? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err(AppError::internal("内核预检查超时，当前内核保持不变"));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        output.seek(SeekFrom::Start(0)).map_err(io_error)?;
        let mut bytes = Vec::new();
        output
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if !status.success() {
            return Err(AppError::internal(format!(
                "内核预检查失败（{status}），当前内核保持不变：{}",
                text.trim()
            )));
        }
        Ok(text)
    })();
    // Also reap failed/timed-out probes. They are never the managed runtime.
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("无法运行候选内核，当前内核保持不变：{error}"))
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
