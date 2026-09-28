use super::Target;
#[cfg(not(target_os = "macos"))]
use crate::errors::AppError;
use crate::errors::AppResult;

pub(super) struct Guard {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    child: std::process::Child,
}
impl Guard {
    pub fn install(target: &Target) -> AppResult<Self> {
        #[cfg(target_os = "macos")]
        {
            crate::services::macos_privilege::set_tun_dns(
                Some(&target.interface),
                Some(&target.server),
                target.pid,
            )?;
            Ok(Self {})
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            use std::io::{BufRead, BufReader};
            use std::process::Stdio;
            let executable =
                std::env::current_exe().map_err(|e| AppError::internal(e.to_string()))?;
            let mut child = crate::services::common::background_command(
                executable
                    .to_str()
                    .ok_or_else(|| AppError::internal("GUI executable path is not UTF-8"))?,
            )
            .args([
                "__tun-dns-helper",
                &target.interface,
                &target.server,
                &target.pid.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError::internal(format!("start DNS guardian: {e}")))?;
            let mut response = String::new();
            let read = BufReader::new(child.stdout.take().unwrap()).read_line(&mut response);
            let result = read
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    serde_json::from_str::<Result<(), String>>(&response).map_err(|e| e.to_string())
                })
                .and_then(|r| r);
            if let Err(error) = result {
                child.stdin.take();
                let _ = child.wait();
                return Err(AppError::internal(error));
            }
            Ok(Self { child })
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = target;
            Err(AppError::internal("当前平台不支持宿主 DNS 接管"))
        }
    }
    pub fn refresh(&mut self, target: &Target) -> AppResult<()> {
        #[cfg(target_os = "macos")]
        {
            crate::services::macos_privilege::set_tun_dns(
                Some(&target.interface),
                Some(&target.server),
                target.pid,
            )
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let _ = target;
            if self
                .child
                .try_wait()
                .map_err(|e| AppError::internal(e.to_string()))?
                .is_some()
            {
                Err(AppError::internal(
                    "TUN DNS guardian exited; retry TUN recovery",
                ))
            } else {
                Ok(())
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = target;
            Err(AppError::internal("当前平台不支持宿主 DNS 接管"))
        }
    }
    pub fn stop(&mut self) -> AppResult<()> {
        #[cfg(target_os = "macos")]
        {
            crate::services::macos_privilege::set_tun_dns(None, None, 0)
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.child.stdin.take();
            let status = self
                .child
                .wait()
                .map_err(|e| AppError::internal(e.to_string()))?;
            if status.success() {
                Ok(())
            } else {
                Err(AppError::internal("TUN DNS cleanup was not confirmed"))
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(AppError::internal("当前平台不支持宿主 DNS 接管"))
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl super::CaptureGuard for Guard {
    fn refresh(&mut self, target: &Target) -> AppResult<()> {
        Self::refresh(self, target)
    }
    fn stop(&mut self) -> AppResult<()> {
        Self::stop(self)
    }
}
