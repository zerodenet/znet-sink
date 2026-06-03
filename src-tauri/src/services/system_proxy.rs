use serde::Serialize;
use crate::services::common;

use crate::errors::{AppError, AppResult};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

pub fn enable(host: &str, port: u16) -> AppResult<SystemProxyStatus> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::invalid_argument("proxy host must not be empty"));
    }
    if port == 0 {
        return Err(AppError::invalid_argument("proxy port must not be zero"));
    }

    set_proxy_platform(host, port, true)?;

    Ok(SystemProxyStatus {
        enabled: true,
        host: host.to_string(),
        port,
    })
}

pub fn disable() -> AppResult<SystemProxyStatus> {
    set_proxy_platform("", 0, false)?;

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
    })
}

pub fn status() -> AppResult<SystemProxyStatus> {
    status_platform()
}

// ── macOS ──

#[cfg(target_os = "macos")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    let services = active_network_services()?;
    if services.is_empty() {
        return Err(AppError::internal(
            "no active network service found; cannot configure system proxy",
        ));
    }

    for service in &services {
        if enable {
            run_networksetup(&["-setwebproxy", service, host, &port.to_string()])?;
            run_networksetup(&["-setsecurewebproxy", service, host, &port.to_string()])?;
        } else {
            run_networksetup(&["-setwebproxystate", service, "off"])?;
            run_networksetup(&["-setsecurewebproxystate", service, "off"])?;
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let services = active_network_services()?;
    for service in &services {
        if let Ok(output) = run_networksetup_output(&["-getwebproxy", service]) {
            if output.contains("Enabled: Yes") {
                // Extract host and port from output
                let host = extract_prop(&output, "Server:")
                    .unwrap_or("127.0.0.1")
                    .to_string();
                let port: u16 = extract_prop(&output, "Port:")
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                return Ok(SystemProxyStatus {
                    enabled: true,
                    host,
                    port,
                });
            }
        }
    }

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
    })
}

#[cfg(target_os = "macos")]
fn active_network_services() -> AppResult<Vec<String>> {
    // List hardware ports to find active network services
    let output = common::background_command("networksetup")
        .args(["-listallhardwareports"])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();
    let mut lines = stdout.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with("Hardware Port:") {
            let service_name = line.trim_start_matches("Hardware Port:").trim();
            // Skip disabled/inactive ports - check next line for Device
            if let Some(device_line) = lines.next() {
                if device_line.contains("Device:") {
                    let device = device_line.trim_start_matches("Device:").trim();
                    // Only include real network interfaces (skip Bluetooth, Thunderbolt bridge, etc.)
                    if device.starts_with("en") || device.starts_with("wl") {
                        services.push(service_name.to_string());
                    }
                }
            }
        }
    }

    Ok(services)
}

#[cfg(target_os = "macos")]
fn run_networksetup(args: &[&str]) -> AppResult<()> {
    let output = common::background_command("networksetup")
        .args(args)
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "networksetup failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_networksetup_output(args: &[&str]) -> AppResult<String> {
    let output = common::background_command("networksetup")
        .args(args)
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn extract_prop<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .lines()
        .find(|line| line.trim().starts_with(key))
        .and_then(|line| line.trim().strip_prefix(key))
        .map(|v| v.trim())
}

// ── Windows ──

#[cfg(target_os = "windows")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    use std::ptr;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_PROXY_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH,
    };

    // Set ProxyEnable via registry
    let output = common::background_command("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            if enable { "1" } else { "0" },
            "/f",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "failed to set Windows ProxyEnable: {}",
            stderr.trim()
        )));
    }

    // Set ProxyServer via registry
    if enable {
        let proxy_server = format!("{host}:{port}");
        let server_output = common::background_command("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                &proxy_server,
                "/f",
            ])
            .output()
            .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

        if !server_output.status.success() {
            let stderr = String::from_utf8_lossy(&server_output.stderr);
            return Err(AppError::internal(format!(
                "failed to set Windows ProxyServer: {}",
                stderr.trim()
            )));
        }
    }

    // Notify system of proxy change
    unsafe {
        InternetSetOptionW(
            ptr::null(),
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            ptr::null(),
            0,
        );
        InternetSetOptionW(ptr::null(), INTERNET_OPTION_REFRESH, ptr::null(), 0);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let output = common::background_command("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to query Windows proxy: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let enabled = stdout.contains("0x1");

    if enabled {
        let server_output = match common::background_command("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyServer",
            ])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                return Err(AppError::internal(format!(
                    "failed to query Windows ProxyServer: {e}"
                )))
            }
        };

        let server_stdout = String::from_utf8_lossy(&server_output.stdout);
        let server = server_stdout
            .lines()
            .find(|l| l.contains("ProxyServer"))
            .and_then(|l| l.split_whitespace().last())
            .unwrap_or("");

        let (host, port) = server
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
            .unwrap_or_default();

        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
        })
    }
}

// ── Linux ──

#[cfg(target_os = "linux")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    let mode = if enable { "manual" } else { "none" };
    let proxy_url = if enable {
        format!("http://{host}:{port}/")
    } else {
        String::new()
    };

    // Try gsettings (GNOME)
    let gsettings_result = common::background_command("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", mode])
        .output();

    if gsettings_result.is_ok() && enable {
        let _ = common::background_command("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", host])
            .output();
        let _ = common::background_command("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &port.to_string(),
            ])
            .output();
        let _ = common::background_command("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", host])
            .output();
        let _ = common::background_command("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &port.to_string(),
            ])
            .output();
    }

    // Also set environment variables for CLI tools
    if enable {
        std::env::set_var("http_proxy", &proxy_url);
        std::env::set_var("https_proxy", &proxy_url);
        std::env::set_var("HTTP_PROXY", &proxy_url);
        std::env::set_var("HTTPS_PROXY", &proxy_url);
    }

    gsettings_result
        .map(|_| ())
        .map_err(|e| AppError::internal(format!("failed to configure Linux proxy: {e}")))
}

#[cfg(target_os = "linux")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let output = common::background_command("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
        .map_err(|e| AppError::internal(format!("failed to query Linux proxy: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let enabled = stdout.contains("manual");

    if enabled {
        let host_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.http", "host"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let host = String::from_utf8_lossy(&host_output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();

        let port_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.http", "port"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let port: u16 = String::from_utf8_lossy(&port_output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);

        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
        })
    }
}
