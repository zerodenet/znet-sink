#[cfg(any(target_os = "macos", test))]
#[path = "system_proxy_macos_bypass.rs"]
mod macos_bypass;

use crate::services::common;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::errors::{AppError, AppResult};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub socks_enabled: bool,
    #[serde(default)]
    pub socks_host: String,
    #[serde(default)]
    pub socks_port: u16,
}

/// Snapshot of the user's original system-proxy configuration, captured
/// immediately before the GUI overrides it. Persisted inside the proxy
/// marker so a later "disable" can *restore* the user's settings instead of
/// blanking them — which is what previously destroyed users' pre-existing
/// proxies (e.g. their own `127.0.0.1:1080`) whenever the kernel stopped or
/// the app exited.
///
/// Windows keeps the original `ProxyServer` string verbatim because it may
/// contain a protocol map with different endpoints. The other optional
/// Windows fields use `None` to mean the registry value was absent before the
/// GUI touched the system and therefore must be absent again after restore.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyBackup {
    /// Whether the OS proxy was enabled before the GUI touched it.
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub socks_enabled: bool,
    #[serde(default)]
    pub socks_host: String,
    #[serde(default)]
    pub socks_port: u16,
    /// Windows `ProxyServer`, preserved verbatim for lossless restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_server: Option<String>,
    /// Windows `ProxyOverride` (bypass list), e.g. `<local>;192.168.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_bypass: Option<String>,
    /// Windows `AutoConfigURL` (PAC script), if configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_config_url: Option<String>,
    /// Original bypass entries per macOS network service, including empty lists.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub macos_bypass: std::collections::BTreeMap<String, Vec<String>>,
    /// Raw GSettings values from before enabling the Linux desktop proxy.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub linux_settings: std::collections::BTreeMap<String, String>,
}

pub fn enable(host: &str, port: u16) -> AppResult<SystemProxyStatus> {
    enable_with_bypass(
        host,
        port,
        &crate::models::app_config::default_proxy_bypass(),
    )
}

pub fn enable_with_bypass(
    host: &str,
    port: u16,
    bypass: &[String],
) -> AppResult<SystemProxyStatus> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::invalid_argument("proxy host must not be empty"));
    }
    if port == 0 {
        return Err(AppError::invalid_argument("proxy port must not be zero"));
    }

    let socks_enabled = supports_socks5(host, port);
    set_proxy_platform(host, port, true, socks_enabled, bypass)?;

    Ok(SystemProxyStatus {
        enabled: true,
        host: host.to_string(),
        port,
        socks_enabled,
        socks_host: if socks_enabled {
            host.to_string()
        } else {
            String::new()
        },
        socks_port: if socks_enabled { port } else { 0 },
    })
}

/// Blank the system proxy unconditionally.
///
/// This is a **destructive** operation — it discards whatever proxy was
/// configured. The GUI lifecycle should normally go through
/// [`crate::services::system_proxy_guard`], which captures a [`ProxyBackup`]
/// on enable and [`restore`]s it on disable, so the user's original settings
/// are recovered instead of being wiped.
pub fn disable() -> AppResult<SystemProxyStatus> {
    set_proxy_platform("", 0, false, false, &[])?;

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
        socks_enabled: false,
        socks_host: String::new(),
        socks_port: 0,
    })
}

pub fn status() -> AppResult<SystemProxyStatus> {
    status_platform()
}

/// Whether the current platform exposes and has the managed local-network
/// bypass protection. `None` means the platform backend cannot inspect it.
pub fn local_bypass_configured() -> Option<bool> {
    local_bypass_configured_platform()
}

/// Read the current OS proxy settings into a [`ProxyBackup`] so they can be
/// restored later. Must be called *before* overwriting anything.
pub fn capture_backup() -> AppResult<ProxyBackup> {
    capture_backup_platform()
}

/// Restore the OS proxy settings from a [`ProxyBackup`] — the inverse of
/// [`capture_backup`]. Used by the proxy guard instead of the destructive
/// [`disable`] so the user's original configuration is recovered.
pub fn restore(backup: &ProxyBackup) -> AppResult<()> {
    restore_platform(backup)
}

fn supports_socks5(host: &str, port: u16) -> bool {
    let timeout = Duration::from_millis(250);
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    for address in addresses {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        if stream.write_all(&[0x05, 0x01, 0x00]).is_err() {
            continue;
        }
        let mut response = [0u8; 2];
        if stream.read_exact(&mut response).is_ok() && response == [0x05, 0x00] {
            return true;
        }
    }
    false
}

#[cfg(any(target_os = "windows", test))]
fn windows_proxy_server(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

#[cfg(any(target_os = "windows", test))]
fn windows_restore_server(backup: &ProxyBackup) -> Option<String> {
    if let Some(server) = backup
        .raw_server
        .as_deref()
        .map(str::trim)
        .filter(|server| !server.is_empty())
    {
        return Some(server.to_string());
    }
    if backup.host.is_empty() {
        return None;
    }
    if backup.socks_enabled {
        Some(format!(
            "http={}:{};https={}:{};socks={}:{}",
            backup.host,
            backup.port,
            backup.host,
            backup.port,
            backup.socks_host,
            backup.socks_port
        ))
    } else {
        Some(windows_proxy_server(&backup.host, backup.port))
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_bypass_equal(actual: Option<&str>, expected: &[String]) -> bool {
    let entries = |value: &str| -> std::collections::BTreeSet<String> {
        value
            .split(';')
            .map(|item| item.trim().to_ascii_lowercase())
            .filter(|item| !item.is_empty())
            .collect()
    };
    entries(actual.unwrap_or_default()) == entries(&expected.join(";"))
}

#[cfg(any(target_os = "linux", test))]
fn linux_bypass_values(bypass: &[String]) -> AppResult<Vec<String>> {
    let mut values = Vec::new();
    for entry in bypass {
        let value = entry.trim();
        if value.eq_ignore_ascii_case("<local>") {
            // GNOME has no Windows-style "all unqualified names" token.
            // The default list also includes localhost explicitly.
            continue;
        }
        let value = value
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .unwrap_or(value);
        let converted = if let Some(prefix) = value.strip_suffix(".*") {
            let parts: Vec<_> = prefix.split('.').collect();
            if parts.is_empty() || parts.len() > 3 {
                return Err(AppError::invalid_argument(format!(
                    "Linux proxy cannot represent bypass pattern: {value}"
                )));
            }
            let octets: Vec<u8> = parts
                .iter()
                .map(|part| part.parse::<u8>())
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    AppError::invalid_argument(format!(
                        "Linux proxy cannot represent bypass pattern: {value}"
                    ))
                })?;
            let mut address = [0u8; 4];
            address[..octets.len()].copy_from_slice(&octets);
            format!("{}/{}", std::net::Ipv4Addr::from(address), octets.len() * 8)
        } else if value.contains('*') && !value.starts_with("*.") {
            return Err(AppError::invalid_argument(format!(
                "Linux proxy cannot represent bypass pattern: {value}"
            )));
        } else {
            value.to_string()
        };
        if !values
            .iter()
            .any(|item: &String| item.eq_ignore_ascii_case(&converted))
        {
            values.push(converted);
        }
    }
    Ok(values)
}

#[cfg(any(target_os = "linux", test))]
fn linux_bypass_literal(values: &[String]) -> String {
    // GVariant accepts JSON-style double-quoted strings when the schema
    // supplies the array-of-strings type, including the empty array.
    serde_json::to_string(values).expect("string list serialization cannot fail")
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_bypass_literal(raw: &str) -> AppResult<Vec<String>> {
    let raw = raw.trim().strip_prefix("@as ").unwrap_or(raw.trim());
    let body = raw
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| AppError::internal("invalid GNOME proxy ignore-hosts value"))?;
    let mut chars = body.chars().peekable();
    let mut values = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(quote @ ('\'' | '"')) = chars.next() else {
            if values.is_empty() && body.trim().is_empty() {
                return Ok(values);
            }
            return Err(AppError::internal("invalid GNOME proxy ignore-hosts value"));
        };
        let mut value = String::new();
        loop {
            match chars.next() {
                Some(c) if c == quote => break,
                Some('\\') => match chars.next() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some(c) => value.push(c),
                    None => {
                        return Err(AppError::internal(
                            "invalid GNOME proxy ignore-hosts escape",
                        ))
                    }
                },
                Some(c) => value.push(c),
                None => {
                    return Err(AppError::internal(
                        "unterminated GNOME proxy ignore-hosts value",
                    ))
                }
            }
        }
        values.push(value);
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.next() {
            None => return Ok(values),
            Some(',') => {
                if chars.peek().is_none() {
                    return Err(AppError::internal("invalid GNOME proxy ignore-hosts value"));
                }
            }
            _ => return Err(AppError::internal("invalid GNOME proxy ignore-hosts value")),
        }
    }
}

// ── macOS ──

#[cfg(target_os = "macos")]
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    socks_enabled: bool,
    bypass: &[String],
) -> AppResult<()> {
    let services = active_network_services()?;
    if services.is_empty() {
        return Err(AppError::internal(
            "no active network service found; cannot configure system proxy",
        ));
    }

    let web_proxy = enable.then_some((host, port));
    let socks_proxy = (enable && socks_enabled).then_some((host, port));
    let mut commands = macos_proxy_commands(&services, web_proxy, socks_proxy);
    if enable {
        let bypass: Vec<_> = bypass
            .iter()
            .filter(|value| value.as_str() != "<local>")
            .map(|value| value.trim_matches(['[', ']']).to_string())
            .collect();
        for service in &services {
            commands.push(macos_bypass::command(service, &bypass));
        }
    }
    run_networksetup_commands(&commands)
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
                let socks = run_networksetup_output(&["-getsocksfirewallproxy", service])
                    .unwrap_or_default();
                let socks_enabled = socks.contains("Enabled: Yes");
                let socks_host = extract_prop(&socks, "Server:").unwrap_or("").to_string();
                let socks_port = extract_prop(&socks, "Port:")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                return Ok(SystemProxyStatus {
                    enabled: true,
                    host,
                    port,
                    socks_enabled,
                    socks_host,
                    socks_port,
                });
            }
        }
    }

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
        socks_enabled: false,
        socks_host: String::new(),
        socks_port: 0,
    })
}

#[cfg(target_os = "macos")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    let mut backup = ProxyBackup {
        enabled: status.enabled,
        host: status.host.clone(),
        port: status.port,
        socks_enabled: status.socks_enabled,
        socks_host: status.socks_host,
        socks_port: status.socks_port,
        raw_server: None,
        override_bypass: None,
        auto_config_url: None,
        macos_bypass: Default::default(),
        linux_settings: Default::default(),
    };
    macos_bypass::capture_missing(&mut backup)?;
    Ok(backup)
}

#[cfg(target_os = "macos")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    let services = active_network_services()?;
    let web = backup
        .enabled
        .then_some((backup.host.as_str(), backup.port));
    let socks = (backup.enabled && backup.socks_enabled)
        .then_some((backup.socks_host.as_str(), backup.socks_port));
    let mut commands = macos_proxy_commands(&services, web, socks);
    for service in &services {
        if let Some(bypass) = backup.macos_bypass.get(service) {
            commands.push(macos_bypass::command(service, bypass));
        }
    }
    run_networksetup_commands(&commands)
}

#[cfg(target_os = "macos")]
fn local_bypass_configured_platform() -> Option<bool> {
    None
}

#[cfg(any(target_os = "macos", test))]
fn parse_network_services(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("An asterisk (*) denotes")
                && !line.starts_with('*')
        })
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "macos")]
fn active_network_services() -> AppResult<Vec<String>> {
    // Proxy commands accept network-service names, not hardware-port names.
    // Those usually match (for example "Wi-Fi") but users can rename a
    // service, so deriving them from `-listallhardwareports` is incorrect.
    let output = common::background_command("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    if !output.status.success() {
        return Err(networksetup_failure(&output));
    }

    Ok(parse_network_services(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(any(target_os = "macos", test))]
fn macos_proxy_commands(
    services: &[String],
    web_proxy: Option<(&str, u16)>,
    socks_proxy: Option<(&str, u16)>,
) -> Vec<Vec<String>> {
    let mut commands = Vec::with_capacity(services.len() * 3);
    for service in services {
        if let Some((host, port)) = web_proxy {
            let port = port.to_string();
            commands.push(vec![
                "-setwebproxy".to_string(),
                service.clone(),
                host.to_string(),
                port.clone(),
            ]);
            commands.push(vec![
                "-setsecurewebproxy".to_string(),
                service.clone(),
                host.to_string(),
                port,
            ]);
        } else {
            commands.push(vec![
                "-setwebproxystate".to_string(),
                service.clone(),
                "off".to_string(),
            ]);
            commands.push(vec![
                "-setsecurewebproxystate".to_string(),
                service.clone(),
                "off".to_string(),
            ]);
        }

        if let Some((host, port)) = socks_proxy {
            commands.push(vec![
                "-setsocksfirewallproxy".to_string(),
                service.clone(),
                host.to_string(),
                port.to_string(),
            ]);
        } else {
            commands.push(vec![
                "-setsocksfirewallproxystate".to_string(),
                service.clone(),
                "off".to_string(),
            ]);
        }
    }
    commands
}

#[cfg(target_os = "macos")]
fn networksetup_failure(output: &std::process::Output) -> AppError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        "no diagnostic output"
    };
    AppError::internal(format!(
        "networksetup failed (status {}): {detail}",
        output.status
    ))
}

#[cfg(any(target_os = "macos", test))]
fn networksetup_requires_admin(stdout: &[u8], stderr: &[u8]) -> bool {
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
    .to_ascii_lowercase();
    diagnostics.contains("requires admin privileges")
        || diagnostics.contains("administrator privileges")
        || diagnostics.contains("not authorized")
}

#[cfg(target_os = "macos")]
fn run_networksetup_elevated(commands: &[Vec<String>]) -> AppResult<()> {
    super::macos_privilege::run_networksetup_commands(commands)
}

#[cfg(target_os = "macos")]
fn run_networksetup_commands(commands: &[Vec<String>]) -> AppResult<()> {
    // After the first authorization, avoid knowingly issuing a command that
    // must fail as the desktop user. Send the complete transaction through
    // the already-authorized, process-lifetime helper instead.
    if super::macos_privilege::has_authorized_helper() {
        return run_networksetup_elevated(commands);
    }

    for (index, arguments) in commands.iter().enumerate() {
        let output = common::background_command("networksetup")
            .args(arguments)
            .output()
            .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

        if output.status.success() {
            continue;
        }
        if networksetup_requires_admin(&output.stdout, &output.stderr) {
            return run_networksetup_elevated(&commands[index..]);
        }
        return Err(networksetup_failure(&output));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_networksetup_output(args: &[&str]) -> AppResult<String> {
    let output = common::background_command("networksetup")
        .args(args)
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    if !output.status.success() {
        return Err(networksetup_failure(&output));
    }

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
const INTERNET_SETTINGS_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[cfg(target_os = "windows")]
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    _socks_enabled: bool,
    bypass: &[String],
) -> AppResult<()> {
    if enable {
        // Windows Settings exposes one manual proxy endpoint. A protocol map
        // such as `http=...;https=...;socks=...` is only partially supported
        // by consumers and is not round-trippable through the Settings UI.
        write_internet_setting_sz("ProxyServer", &windows_proxy_server(host, port))?;

        // Never send loopback or LAN traffic back into the local mixed
        // listener. This also protects adapters that appear after Wi-Fi is
        // enabled while the application proxy is already active.
        if bypass.is_empty() {
            delete_internet_setting("ProxyOverride");
        } else {
            write_internet_setting_sz("ProxyOverride", &bypass.join(";"))?;
        }

        // A PAC script can take precedence over the manual proxy for some
        // clients. The guard has already backed it up, so remove it while our
        // proxy is active and restore it on disconnect.
        delete_internet_setting("AutoConfigURL");

        // Enable last so Windows never observes the old server with the new
        // enabled state during the registry update.
        write_internet_setting_dword("ProxyEnable", 1)?;
    } else {
        write_internet_setting_dword("ProxyEnable", 0)?;
    }

    notify_settings_changed();
    if enable {
        let actual = status_platform()?;
        if !actual.enabled
            || actual.host != host
            || actual.port != port
            || !bypass_matches(bypass)?
            || query_internet_setting("AutoConfigURL").is_some()
        {
            return Err(AppError::internal(
                "Windows proxy settings did not match after setting them",
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let enabled = query_internet_setting("ProxyEnable")
        .map(|v| v.trim() == "0x1")
        .unwrap_or(false);

    if enabled {
        let server = query_internet_setting("ProxyServer").unwrap_or_default();
        let (host, port) = parse_server(&server);
        let (socks_host, socks_port) = parse_named_server(&server, "socks");
        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
            socks_enabled: server.to_ascii_lowercase().contains("socks="),
            socks_host,
            socks_port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
            socks_enabled: false,
            socks_host: String::new(),
            socks_port: 0,
        })
    }
}

#[cfg(target_os = "windows")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let enabled = query_internet_setting("ProxyEnable")
        .map(|v| v.trim() == "0x1")
        .unwrap_or(false);
    let server = query_internet_setting("ProxyServer").unwrap_or_default();
    let (host, port) = parse_server(&server);
    let (socks_host, socks_port) = parse_named_server(&server, "socks");
    Ok(ProxyBackup {
        enabled,
        host,
        port,
        socks_enabled: !socks_host.is_empty(),
        socks_host,
        socks_port,
        raw_server: (!server.trim().is_empty()).then_some(server),
        override_bypass: query_internet_setting("ProxyOverride"),
        auto_config_url: query_internet_setting("AutoConfigURL"),
        macos_bypass: Default::default(),
        linux_settings: Default::default(),
    })
}

#[cfg(target_os = "windows")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    // Restore values before ProxyEnable so clients cannot briefly use a stale
    // server while the original state is being reconstructed.
    if let Some(server) = windows_restore_server(backup) {
        write_internet_setting_sz("ProxyServer", &server)?;
    } else {
        delete_internet_setting("ProxyServer");
    }

    if let Some(bypass) = &backup.override_bypass {
        write_internet_setting_sz("ProxyOverride", bypass)?;
    } else {
        delete_internet_setting("ProxyOverride");
    }

    if let Some(url) = &backup.auto_config_url {
        write_internet_setting_sz("AutoConfigURL", url)?;
    } else {
        delete_internet_setting("AutoConfigURL");
    }

    write_internet_setting_dword("ProxyEnable", if backup.enabled { 1 } else { 0 })?;
    notify_settings_changed();
    if status_platform()?.enabled != backup.enabled
        || query_internet_setting("ProxyServer") != windows_restore_server(backup)
        || query_internet_setting("ProxyOverride") != backup.override_bypass
        || query_internet_setting("AutoConfigURL") != backup.auto_config_url
    {
        return Err(AppError::internal(
            "Windows proxy settings did not match after restoring them",
        ));
    }
    Ok(())
}

/// Query a single value from the Internet Settings registry key.
/// Returns `None` if the value is absent or unreadable.
#[cfg(target_os = "windows")]
fn query_internet_setting(value_name: &str) -> Option<String> {
    let output = common::background_command("reg")
        .args(["query", INTERNET_SETTINGS_KEY, "/v", value_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A matching line looks like:
    //   "    ProxyServer    REG_SZ    127.0.0.1:1080"
    //   "    ProxyEnable    REG_DWORD    0x1"
    for line in stdout.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(value_name) else {
            continue;
        };
        let rest = rest.trim_start();
        // rest = "REG_SZ    <value>" — skip the type token to get the value.
        let kind_end = rest.find(' ')?;
        return Some(rest[kind_end..].trim().to_string());
    }
    None
}

#[cfg(target_os = "windows")]
fn local_bypass_configured_platform() -> Option<bool> {
    let bypass = query_internet_setting("ProxyOverride")?.to_ascii_lowercase();
    Some(bypass.contains("<local>") && bypass.contains("localhost") && bypass.contains("127.*"))
}

#[cfg(target_os = "windows")]
fn write_internet_setting_dword(value_name: &str, value: u32) -> AppResult<()> {
    let output = common::background_command("reg")
        .args([
            "add",
            INTERNET_SETTINGS_KEY,
            "/v",
            value_name,
            "/t",
            "REG_DWORD",
            "/d",
            &value.to_string(),
            "/f",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "failed to set Windows {}: {}",
            value_name,
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn write_internet_setting_sz(value_name: &str, value: &str) -> AppResult<()> {
    let output = common::background_command("reg")
        .args([
            "add",
            INTERNET_SETTINGS_KEY,
            "/v",
            value_name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "failed to set Windows {}: {}",
            value_name,
            stderr.trim()
        )));
    }
    Ok(())
}

/// Best-effort deletion of a single value. Silently ignored if the value
/// is absent (which is the common case we want to tolerate on restore).
#[cfg(target_os = "windows")]
fn delete_internet_setting(value_name: &str) {
    let _ = common::background_command("reg")
        .args(["delete", INTERNET_SETTINGS_KEY, "/v", value_name, "/f"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(target_os = "windows")]
fn notify_settings_changed() {
    use std::ptr;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_PROXY_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH,
    };

    unsafe {
        InternetSetOptionW(
            ptr::null(),
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            ptr::null(),
            0,
        );
        InternetSetOptionW(ptr::null(), INTERNET_OPTION_REFRESH, ptr::null(), 0);
    }
}

#[cfg(target_os = "windows")]
fn parse_server(server: &str) -> (String, u16) {
    if let Some((_, value)) = server.split(';').find_map(|item| item.split_once('=')) {
        return value
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
            .unwrap_or_default();
    }
    server
        .split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn parse_named_server(server: &str, name: &str) -> (String, u16) {
    server
        .split(';')
        .filter_map(|item| item.split_once('='))
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.split_once(':'))
        .map(|(host, port)| (host.to_string(), port.parse().unwrap_or(0)))
        .unwrap_or_default()
}

// ── Linux ──

#[cfg(target_os = "linux")]
const LINUX_PROXY_SETTINGS: &[(&str, &str)] = &[
    ("org.gnome.system.proxy", "mode"),
    ("org.gnome.system.proxy", "ignore-hosts"),
    ("org.gnome.system.proxy.http", "host"),
    ("org.gnome.system.proxy.http", "port"),
    ("org.gnome.system.proxy.https", "host"),
    ("org.gnome.system.proxy.https", "port"),
    ("org.gnome.system.proxy.socks", "host"),
    ("org.gnome.system.proxy.socks", "port"),
];

#[cfg(target_os = "linux")]
fn linux_setting_key(schema: &str, key: &str) -> String {
    format!("{schema}/{key}")
}

#[cfg(target_os = "linux")]
fn gsettings_output(
    operation: &str,
    schema: &str,
    key: &str,
    value: Option<&str>,
) -> AppResult<String> {
    let mut command = common::background_command("gsettings");
    command.args([operation, schema, key]);
    if let Some(value) = value {
        command.arg(value);
    }
    let output = command.output().map_err(|error| {
        AppError::internal(format!(
            "failed to run gsettings {operation} {schema} {key}: {error}"
        ))
    })?;
    checked_gsettings_output(operation, schema, key, output)
}

#[cfg(any(target_os = "linux", test))]
fn checked_gsettings_output(
    operation: &str,
    schema: &str,
    key: &str,
    output: std::process::Output,
) -> AppResult<String> {
    if !output.status.success() {
        return Err(AppError::internal(format!(
            "gsettings {operation} {schema} {key} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn gsettings_get(schema: &str, key: &str) -> AppResult<String> {
    gsettings_output("get", schema, key, None)
}

#[cfg(target_os = "linux")]
fn gsettings_set(schema: &str, key: &str, value: &str) -> AppResult<()> {
    gsettings_output("set", schema, key, Some(value)).map(|_| ())
}

#[cfg(target_os = "linux")]
fn set_linux_proxy_fields(
    host: &str,
    port: u16,
    socks_enabled: bool,
    bypass: Option<&[String]>,
) -> AppResult<()> {
    for schema in [
        "org.gnome.system.proxy.http",
        "org.gnome.system.proxy.https",
    ] {
        gsettings_set(schema, "host", host)?;
        gsettings_set(schema, "port", &port.to_string())?;
    }
    let socks = "org.gnome.system.proxy.socks";
    gsettings_set(socks, "host", if socks_enabled { host } else { "" })?;
    gsettings_set(
        socks,
        "port",
        &if socks_enabled { port } else { 0 }.to_string(),
    )?;
    if let Some(bypass) = bypass {
        let values = linux_bypass_values(bypass)?;
        gsettings_set(
            "org.gnome.system.proxy",
            "ignore-hosts",
            &linux_bypass_literal(&values),
        )?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    socks_enabled: bool,
    bypass: &[String],
) -> AppResult<()> {
    if enable {
        // Apply all fields before switching GNOME into manual mode.
        set_linux_proxy_fields(host, port, socks_enabled, Some(bypass))?;
    }
    let desired_mode = if enable { "manual" } else { "none" };
    gsettings_set("org.gnome.system.proxy", "mode", desired_mode)?;
    let actual_mode = gsettings_get("org.gnome.system.proxy", "mode")?;
    if actual_mode.trim_matches('\'') != desired_mode {
        return Err(AppError::internal(
            "Linux proxy mode did not match after setting it",
        ));
    }
    if enable {
        let actual = status_platform()?;
        if actual.host != host
            || actual.port != port
            || actual.socks_enabled != socks_enabled
            || (socks_enabled && (actual.socks_host != host || actual.socks_port != port))
            || !bypass_matches(bypass)?
        {
            return Err(AppError::internal(
                "Linux proxy settings did not match after setting them",
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let mode = gsettings_get("org.gnome.system.proxy", "mode")?;
    let enabled = mode.trim_matches('\'') == "manual";

    if enabled {
        let host = gsettings_get("org.gnome.system.proxy.http", "host")?
            .trim_matches('\'')
            .to_string();
        let port = gsettings_get("org.gnome.system.proxy.http", "port")?
            .parse::<u16>()
            .map_err(|error| {
                AppError::internal(format!("invalid Linux HTTP proxy port: {error}"))
            })?;
        let socks_host = gsettings_get("org.gnome.system.proxy.socks", "host")?
            .trim_matches('\'')
            .to_string();
        let socks_port = gsettings_get("org.gnome.system.proxy.socks", "port")?
            .parse::<u16>()
            .map_err(|error| {
                AppError::internal(format!("invalid Linux SOCKS proxy port: {error}"))
            })?;

        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
            socks_enabled: !socks_host.is_empty() && socks_port != 0,
            socks_host,
            socks_port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
            socks_enabled: false,
            socks_host: String::new(),
            socks_port: 0,
        })
    }
}

#[cfg(target_os = "linux")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    let mut linux_settings = std::collections::BTreeMap::new();
    for (schema, key) in LINUX_PROXY_SETTINGS {
        linux_settings.insert(linux_setting_key(schema, key), gsettings_get(schema, key)?);
    }
    Ok(ProxyBackup {
        enabled: status.enabled,
        host: status.host.clone(),
        port: status.port,
        socks_enabled: status.socks_enabled,
        socks_host: status.socks_host,
        socks_port: status.socks_port,
        raw_server: None,
        override_bypass: None,
        auto_config_url: None,
        macos_bypass: Default::default(),
        linux_settings,
    })
}

#[cfg(target_os = "linux")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    if !backup.linux_settings.is_empty() {
        for (schema, key) in LINUX_PROXY_SETTINGS {
            if *key == "mode" {
                continue;
            }
            if let Some(value) = backup.linux_settings.get(&linux_setting_key(schema, key)) {
                gsettings_set(schema, key, value)?;
            }
        }
        let mode = backup
            .linux_settings
            .get(&linux_setting_key("org.gnome.system.proxy", "mode"))
            .ok_or_else(|| AppError::internal("Linux proxy backup is missing its original mode"))?;
        gsettings_set("org.gnome.system.proxy", "mode", mode)?;
        for (schema, key) in LINUX_PROXY_SETTINGS {
            if let Some(expected) = backup.linux_settings.get(&linux_setting_key(schema, key)) {
                if gsettings_get(schema, key)? != *expected {
                    return Err(AppError::internal(format!(
                        "Linux proxy setting {schema}/{key} did not restore"
                    )));
                }
            }
        }
        Ok(())
    } else {
        // Legacy markers did not capture ignore-hosts. Leave that key alone.
        if backup.enabled {
            set_linux_proxy_fields(&backup.host, backup.port, backup.socks_enabled, None)?;
        }
        gsettings_set(
            "org.gnome.system.proxy",
            "mode",
            if backup.enabled { "manual" } else { "none" },
        )
    }
}

#[cfg(target_os = "linux")]
fn local_bypass_configured_platform() -> Option<bool> {
    let actual = gsettings_get("org.gnome.system.proxy", "ignore-hosts").ok()?;
    let actual = parse_linux_bypass_literal(&actual).ok()?;
    Some(["localhost", "127.0.0.0/8", "::1"].iter().all(|required| {
        actual
            .iter()
            .any(|value| value.eq_ignore_ascii_case(required))
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        checked_gsettings_output, linux_bypass_literal, linux_bypass_values, macos_proxy_commands,
        networksetup_requires_admin, parse_linux_bypass_literal, parse_network_services,
        windows_bypass_equal, windows_proxy_server, windows_restore_server, ProxyBackup,
    };

    #[test]
    fn macos_network_services_use_service_names_and_skip_disabled_entries() {
        let output = "An asterisk (*) denotes that a network service is disabled.\nHome Wi-Fi\n*USB LAN\nOffice Ethernet\n";

        assert_eq!(
            parse_network_services(output),
            vec!["Home Wi-Fi".to_string(), "Office Ethernet".to_string()]
        );
    }

    #[test]
    fn macos_admin_diagnostic_triggers_native_authorization_retry() {
        assert!(networksetup_requires_admin(
            b"",
            b"Error: Command requires admin privileges."
        ));
    }

    #[test]
    fn macos_proxy_changes_stay_as_structured_arguments() {
        let commands = macos_proxy_commands(
            &["Home Wi-Fi's && touch /tmp/not-created".to_string()],
            Some(("127.0.0.1", 7890)),
            Some(("127.0.0.1", 7890)),
        );

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0][1], "Home Wi-Fi's && touch /tmp/not-created");
        assert_eq!(commands[0][2], "127.0.0.1");
        assert_eq!(commands[0][3], "7890");
    }

    #[test]
    fn windows_manual_proxy_uses_settings_compatible_endpoint() {
        assert_eq!(windows_proxy_server("127.0.0.1", 7890), "127.0.0.1:7890");
    }

    #[test]
    fn windows_restore_preserves_protocol_map_verbatim() {
        let backup = ProxyBackup {
            raw_server: Some(
                "http=127.0.0.1:8080;https=127.0.0.1:8443;socks=127.0.0.1:1080".to_string(),
            ),
            ..ProxyBackup::default()
        };

        assert_eq!(
            windows_restore_server(&backup).as_deref(),
            Some("http=127.0.0.1:8080;https=127.0.0.1:8443;socks=127.0.0.1:1080")
        );
    }

    #[test]
    fn windows_restore_supports_legacy_markers() {
        let backup = ProxyBackup {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 1080,
            ..ProxyBackup::default()
        };

        assert_eq!(
            windows_restore_server(&backup).as_deref(),
            Some("127.0.0.1:1080")
        );
    }

    #[test]
    fn windows_bypass_readback_compares_values_instead_of_assuming_success() {
        let expected = vec!["<local>".into(), "127.*".into(), "*.example.org".into()];
        assert!(windows_bypass_equal(
            Some(" 127.* ; *.EXAMPLE.ORG ; <LOCAL> "),
            &expected
        ));
        assert!(!windows_bypass_equal(Some("<local>;127.*"), &expected));
        assert!(!windows_bypass_equal(None, &expected));
        assert!(windows_bypass_equal(None, &[]));
    }

    #[test]
    fn linux_bypass_uses_gnome_network_ranges_and_round_trips_literals() {
        let bypass = vec![
            "<local>".into(),
            "localhost".into(),
            "127.*".into(),
            "[::1]".into(),
            "192.168.*".into(),
            "172.31.*".into(),
            "*.example.org".into(),
        ];
        let values = linux_bypass_values(&bypass).unwrap();
        assert_eq!(
            values,
            [
                "localhost",
                "127.0.0.0/8",
                "::1",
                "192.168.0.0/16",
                "172.31.0.0/16",
                "*.example.org"
            ]
        );
        assert_eq!(
            parse_linux_bypass_literal(&linux_bypass_literal(&values)).unwrap(),
            values
        );
        assert_eq!(
            parse_linux_bypass_literal("@as []").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_linux_bypass_literal("['localhost', '127.0.0.0/8']").unwrap(),
            ["localhost", "127.0.0.0/8"]
        );
        assert!(linux_bypass_values(&["example.*".into()]).is_err());
    }

    #[test]
    fn linux_backup_preserves_raw_gsettings_and_old_markers_remain_readable() {
        let mut backup = ProxyBackup::default();
        backup.linux_settings.insert(
            "org.gnome.system.proxy/ignore-hosts".into(),
            "['custom.example', '::1']".into(),
        );
        backup
            .linux_settings
            .insert("org.gnome.system.proxy/mode".into(), "'auto'".into());
        let encoded = serde_json::to_string(&backup).unwrap();
        let restored: ProxyBackup = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.linux_settings, backup.linux_settings);
        let old: ProxyBackup =
            serde_json::from_str(r#"{"enabled":false,"host":"","port":0}"#).unwrap();
        assert!(old.linux_settings.is_empty());
    }

    #[test]
    fn linux_gsettings_nonzero_exit_is_an_error() {
        let output = std::process::Command::new("rustc")
            .arg("--invalid-proxy-test-option")
            .output()
            .unwrap();
        assert!(checked_gsettings_output("set", "org.gnome.system.proxy", "mode", output).is_err());
    }
}

/// Capture only previously untouched service lists before extending ownership.
pub fn complete_bypass_backup(backup: &mut ProxyBackup) -> AppResult<bool> {
    #[cfg(target_os = "macos")]
    {
        macos_bypass::capture_missing(backup)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = backup;
        Ok(false)
    }
}

/// Read native exceptions before taking the idempotent-enable fast path.
pub fn bypass_matches(bypass: &[String]) -> AppResult<bool> {
    #[cfg(target_os = "macos")]
    {
        let expected: std::collections::BTreeSet<_> = bypass
            .iter()
            .filter(|v| v.as_str() != "<local>")
            .map(|v| v.trim_matches(['[', ']']).to_ascii_lowercase())
            .collect();
        for service in active_network_services()? {
            let output = run_networksetup_output(&["-getproxybypassdomains", &service])?;
            let actual: std::collections::BTreeSet<_> = macos_bypass::parse(&output)
                .into_iter()
                .map(|v| v.to_ascii_lowercase())
                .collect();
            if actual != expected {
                return Ok(false);
            }
        }
        Ok(true)
    }
    #[cfg(target_os = "windows")]
    {
        Ok(windows_bypass_equal(
            query_internet_setting("ProxyOverride").as_deref(),
            bypass,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let expected = linux_bypass_values(bypass)?;
        let actual = gsettings_get("org.gnome.system.proxy", "ignore-hosts")?;
        let actual = parse_linux_bypass_literal(&actual)?;
        let canonical = |entries: Vec<String>| -> std::collections::BTreeSet<String> {
            entries
                .into_iter()
                .map(|entry| entry.to_ascii_lowercase())
                .collect()
        };
        Ok(canonical(actual) == canonical(expected))
    }
}
