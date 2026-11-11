//! Read-only admission checks. Never delete an interface or route by its name.
#[cfg(any(windows, test))]
use crate::errors::AppError;
use crate::errors::AppResult;
#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};

#[cfg(any(windows, test))]
fn blocked(message: impl Into<String>) -> AppError {
    AppError {
        code: "tun_cleanup_unconfirmed",
        message: message.into(),
        details: None,
    }
}

#[cfg(windows)]
pub(crate) fn confirm_absent(
    tun: &crate::models::app_config::AppTunConfig,
    working_dir: Option<&str>,
) -> AppResult<()> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("ZERO_TUN_STATE_DIR") {
        let root = PathBuf::from(root);
        roots.push(if root.is_absolute() {
            root
        } else {
            working_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(root)
        });
    } else {
        roots.push(
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("Zero")
                .join("run"),
        );
        roots.push(std::env::temp_dir().join("zero"));
    }
    wait_absent(std::time::Duration::from_secs(5), || {
        audit(
            tun.name.as_deref().unwrap_or("ZeroTun"),
            &roots,
            adapter_present,
        )
    })
}

#[cfg(any(windows, test))]
fn wait_absent(
    timeout: std::time::Duration,
    mut probe: impl FnMut() -> AppResult<()>,
) -> AppResult<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match probe() {
            Ok(()) => return Ok(()),
            Err(error) if std::time::Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn confirm_absent(
    _: &crate::models::app_config::AppTunConfig,
    _: Option<&str>,
) -> AppResult<()> {
    Ok(())
}

#[cfg(any(windows, test))]
fn audit(
    name: &str,
    roots: &[PathBuf],
    inspect: impl FnOnce(&str) -> AppResult<bool>,
) -> AppResult<()> {
    if inspect(name)? {
        return Err(blocked(format!(
            "old TUN adapter `{name}` is still present; automatic replacement is blocked"
        )));
    }
    for root in roots {
        check_journals(root)?;
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn check_journals(root: &Path) -> AppResult<()> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(blocked(format!(
                "cannot inspect TUN route recovery state: {error}"
            )))
        }
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| blocked(format!("cannot inspect TUN route journal: {error}")))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // The kernel clears its recovery journals only after all recorded
        // routes/exclusions are removed. Even a malformed/empty journal is an
        // outstanding recovery obligation, never evidence of successful cleanup.
        if name.starts_with("routes-") && name.ends_with(".json") {
            return Err(blocked(format!(
                "TUN route recovery journal remains at {}; automatic replacement is blocked",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn adapter_present(name: &str) -> AppResult<bool> {
    use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, MIB_IF_TABLE2,
    };
    struct Table(*mut MIB_IF_TABLE2);
    impl Drop for Table {
        fn drop(&mut self) {
            unsafe { FreeMibTable(self.0.cast()) }
        }
    }
    let mut raw = std::ptr::null_mut();
    let code = unsafe { GetIfTable2(&mut raw) };
    if code != 0 {
        return Err(blocked(format!(
            "cannot enumerate Windows TUN interfaces: {}",
            std::io::Error::from_raw_os_error(code as i32)
        )));
    }
    if raw.is_null() {
        return Err(blocked("Windows returned no interface audit table"));
    }
    let table = Table(raw);
    let rows = unsafe {
        std::slice::from_raw_parts((*table.0).Table.as_ptr(), (*table.0).NumEntries as usize)
    };
    let expected: Vec<u16> = name.encode_utf16().collect();
    let expected_len =
        i32::try_from(expected.len()).map_err(|_| blocked("TUN interface name is too long"))?;
    for row in rows {
        let end = row
            .Alias
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(row.Alias.len());
        let comparison = unsafe {
            CompareStringOrdinal(
                row.Alias.as_ptr(),
                end as i32,
                expected.as_ptr(),
                expected_len,
                1,
            )
        };
        if comparison == 0 {
            return Err(blocked(format!(
                "cannot compare Windows TUN interface names: {}",
                std::io::Error::last_os_error()
            )));
        }
        if comparison == CSTR_EQUAL {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "residue_tests.rs"]
mod tests;
