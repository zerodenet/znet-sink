use super::*;

/// Re-read ownership immediately before clearing. macOS services/protocols may
/// have different proxies; never disable a channel that now belongs elsewhere.
pub(crate) fn clear_matching(endpoints: &[(String, u16)]) -> AppResult<()> {
    mutate_proxy(|| clear_platform(endpoints))
}

fn matches(host: &str, port: u16, endpoints: &[(String, u16)]) -> bool {
    endpoints.iter().any(|(h, p)| host == h && port == *p)
}

#[cfg(target_os = "macos")]
fn clear_platform(endpoints: &[(String, u16)]) -> AppResult<()> {
    let mut commands = Vec::new();
    for service in active_network_services()? {
        for (read, write) in [
            ("-getwebproxy", "-setwebproxystate"),
            ("-getsecurewebproxy", "-setsecurewebproxystate"),
            ("-getsocksfirewallproxy", "-setsocksfirewallproxystate"),
        ] {
            let output = run_networksetup_output(&[read, &service])?;
            if let Some(command) = matching_macos_command(&service, write, &output, endpoints)? {
                commands.push(command);
            }
        }
    }
    if commands.is_empty() {
        return Ok(());
    }
    run_networksetup_commands(&commands)
}

#[cfg(any(target_os = "macos", test))]
fn matching_macos_command(
    service: &str,
    write: &str,
    output: &str,
    endpoints: &[(String, u16)],
) -> AppResult<Option<Vec<String>>> {
    let invalid = || AppError::internal("cannot verify current macOS proxy ownership");
    match extract_prop(output, "Enabled:") {
        Some("No") => return Ok(None),
        Some("Yes") => {}
        _ => return Err(invalid()),
    }
    let host = extract_prop(output, "Server:").ok_or_else(invalid)?;
    let port = extract_prop(output, "Port:")
        .and_then(|value| value.parse().ok())
        .ok_or_else(invalid)?;
    Ok(matches(host, port, endpoints).then(|| vec![write.into(), service.into(), "off".into()]))
}

#[cfg(not(target_os = "macos"))]
fn clear_platform(endpoints: &[(String, u16)]) -> AppResult<()> {
    let current = status_platform()?;
    if current.enabled && matches(&current.host, current.port, endpoints) {
        if current.socks_enabled && !matches(&current.socks_host, current.socks_port, endpoints) {
            return Err(AppError::internal(
                "proxy channels have mixed ownership; cleanup skipped",
            ));
        }
        #[cfg(target_os = "windows")]
        {
            let server = query_internet_setting("ProxyServer").unwrap_or_default();
            for entry in server.split(';').filter(|entry| !entry.trim().is_empty()) {
                let value = entry
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or(entry);
                let (host, port) = parse_server(value);
                if !matches(&host, port, endpoints) {
                    return Err(AppError::internal(
                        "proxy channels have mixed ownership; cleanup skipped",
                    ));
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let host = gsettings_get("org.gnome.system.proxy.https", "host")?;
            let port = gsettings_get("org.gnome.system.proxy.https", "port")?
                .parse::<u16>()
                .map_err(|_| AppError::internal("invalid HTTPS proxy port"))?;
            let host = host.trim_matches('\'');
            if !host.is_empty() && port != 0 && !matches(host, port, endpoints) {
                return Err(AppError::internal(
                    "proxy channels have mixed ownership; cleanup skipped",
                ));
            }
        }
        set_proxy_platform("", 0, false, false, &[])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "owned_cleanup_tests.rs"]
mod tests;
