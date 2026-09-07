use crate::errors::{AppError, AppResult};
use std::net::IpAddr;

pub(super) fn network(rule: &str) -> AppResult<Option<String>> {
    let ip_text = rule.trim_matches(['[', ']']);
    if let Ok(ip) = ip_text.parse::<IpAddr>() {
        return Ok(Some(format!(
            "{ip}/{}",
            if ip.is_ipv4() { 32 } else { 128 }
        )));
    }
    let cidr = if rule.contains('/') {
        rule.to_string()
    } else if rule.ends_with(".*") {
        let parts: Vec<_> = rule.trim_end_matches(".*").split('.').collect();
        if parts.iter().all(|part| part.parse::<u8>().is_ok()) && parts.len() < 4 {
            let mut octets = parts.clone();
            octets.resize(4, "0");
            format!("{}/{}", octets.join("."), parts.len() * 8)
        } else {
            return Ok(None);
        }
    } else {
        return Ok(None);
    };
    let (ip, prefix) = crate::services::kernel_settings::validate_cidr(&cidr, "绕过规则")?;
    let ip: IpAddr = match ip {
        IpAddr::V4(ip) => std::net::Ipv4Addr::from(
            u32::from(ip)
                & if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                },
        )
        .into(),
        IpAddr::V6(ip) => std::net::Ipv6Addr::from(
            u128::from(ip)
                & if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                },
        )
        .into(),
    };
    Ok(Some(format!("{ip}/{prefix}")))
}

pub(super) fn native_patterns(rule: &str) -> Vec<String> {
    let Ok(Some(network)) = network(rule) else {
        return vec![rule.to_string()];
    };
    let (ip, prefix) = network.split_once('/').unwrap();
    let prefix: u8 = prefix.parse().unwrap();
    if ip.contains(':') {
        return if prefix == 128 {
            vec![format!("[{ip}]")]
        } else {
            vec![]
        };
    }
    let octets: Vec<_> = ip.split('.').collect();
    match prefix {
        8 | 16 | 24 => vec![format!("{}.*", octets[..(prefix / 8) as usize].join("."))],
        32 => vec![ip.to_string()],
        // The default private /12 has a small exact representation on all platforms.
        12 if ip == "172.16.0.0" => (16..32).map(|n| format!("172.{n}.*")).collect(),
        _ => vec![],
    }
}

pub(super) fn domain_pattern(rule: &str) -> AppResult<String> {
    if rule == "<local>" {
        return Ok("^[^.]+$".to_string());
    }
    if rule.is_empty()
        || rule.starts_with('-')
        || (rule.contains(['*', '?'])
            && rule
                .chars()
                .all(|c| c.is_ascii_digit() || ".*?".contains(c)))
        || rule == "*"
        || !rule
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_*?".contains(c))
    {
        return Err(AppError::invalid_argument(format!(
            "无效绕过规则 `{rule}`：请填写 IP、CIDR 或域名模式"
        )));
    }
    let mut result = String::from("(?i)^");
    for c in rule.chars() {
        match c {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' => result.push_str("\\."),
            _ => result.push(c),
        }
    }
    result.push('$');
    Ok(result)
}
