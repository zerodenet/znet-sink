use super::*;

pub(super) fn parse(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("There aren't any bypass domains set on ")
        })
        .map(str::to_owned)
        .collect()
}

pub(super) fn command(service: &str, bypass: &[String]) -> Vec<String> {
    let mut command = vec!["-setproxybypassdomains".into(), service.into()];
    if bypass.is_empty() {
        command.push("Empty".into());
    } else {
        command.extend_from_slice(bypass);
    }
    command
}

#[cfg(target_os = "macos")]
pub(super) fn capture_missing(backup: &mut ProxyBackup) -> AppResult<bool> {
    let mut changed = false;
    for service in active_network_services()? {
        if !backup.macos_bypass.contains_key(&service) {
            let output = run_networksetup_output(&["-getproxybypassdomains", &service])?;
            backup.macos_bypass.insert(service, parse(&output));
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_status_is_not_restored_as_a_domain_and_empty_uses_native_sentinel() {
        assert!(parse("There aren't any bypass domains set on USB LAN.\n").is_empty());
        assert_eq!(
            command("USB LAN", &[]),
            ["-setproxybypassdomains", "USB LAN", "Empty"]
        );
        assert_eq!(parse("localhost\n192.168.*\n"), ["localhost", "192.168.*"]);
    }

    #[test]
    fn service_names_and_domains_remain_separate_arguments() {
        assert_eq!(
            command("USB LAN", &["192.168.*".into(), "*.example.org".into()]),
            [
                "-setproxybypassdomains",
                "USB LAN",
                "192.168.*",
                "*.example.org"
            ]
        );
    }
}
