//! WireGuard authoring validation; protocol-specific fields stay in this adapter.
use super::invalid;
use crate::errors::AppResult;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{collections::HashSet, net::IpAddr};
fn cidr(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|s| s.split_once('/'))
        .is_some_and(
            |(ip, prefix)| match (ip.parse::<IpAddr>(), prefix.parse::<u8>()) {
                (Ok(IpAddr::V4(_)), Ok(prefix)) => prefix <= 32,
                (Ok(IpAddr::V6(_)), Ok(prefix)) => prefix <= 128,
                _ => false,
            },
        )
}
fn key(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|s| STANDARD.decode(s).ok())
        .is_some_and(|bytes| bytes.len() == 32)
}
pub(super) fn dns_uses_endpoint(row: &Value, address: IpAddr) -> bool {
    row["protocol"]["peers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|peer| peer["allowed_ips"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .any(|network| {
            let Some((ip, prefix)) = network.split_once('/') else {
                return false;
            };
            match (ip.parse::<IpAddr>(), address, prefix.parse::<u32>()) {
                (Ok(IpAddr::V4(ip)), IpAddr::V4(address), Ok(prefix)) if prefix <= 32 => {
                    let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
                    u32::from(ip) & mask == u32::from(address) & mask
                }
                (Ok(IpAddr::V6(ip)), IpAddr::V6(address), Ok(prefix)) if prefix <= 128 => {
                    let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
                    u128::from(ip) & mask == u128::from(address) & mask
                }
                _ => false,
            }
        })
}
pub(super) fn validate(row: &Value) -> AppResult<()> {
    let protocol = &row["protocol"];
    if !key(&protocol["private_key"]) {
        return Err(invalid("WireGuard 私钥必须为 Base64 编码的 32 字节密钥"));
    }
    if !protocol["addresses"]
        .as_array()
        .is_some_and(|v| !v.is_empty() && v.iter().all(cidr))
    {
        return Err(invalid(
            "WireGuard 本机隧道地址请填写 Address 中的 IP/前缀，例如 10.66.0.2/32；不能填写密钥",
        ));
    }
    if let Some(mtu) = protocol.get("mtu") {
        let ipv6 = protocol["addresses"].as_array().is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_str().is_some_and(|value| value.contains(':')))
        });
        let min = if ipv6 { 1280 } else { 68 };
        if !mtu
            .as_u64()
            .is_some_and(|value| (min..=65488).contains(&value))
        {
            return Err(invalid(
                "WireGuard MTU 超出允许范围（IPv4 最小 68，IPv6 最小 1280，最大 65488）",
            ));
        }
    }
    let peers = protocol["peers"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("WireGuard 至少需要一个 peer"))?;
    let mut public_keys = HashSet::new();
    for peer in peers {
        if row["directions"]["outbound"] != false
            && row.get("listen").is_none()
            && peer.get("endpoint").is_none()
        {
            return Err(invalid(
                "仅启用出站时请为每个 Peer 配置远端地址（Endpoint）",
            ));
        }
        if !key(&peer["public_key"]) {
            return Err(invalid("Peer 公钥必须为 Base64 编码的 32 字节密钥"));
        }
        if !public_keys.insert(peer["public_key"].as_str().unwrap()) {
            return Err(invalid("Peer 公钥不能重复"));
        }
        if peer
            .get("pre_shared_key")
            .is_some_and(|value| !value.is_null() && !key(value))
        {
            return Err(invalid("Peer 预共享密钥必须为 Base64 编码的 32 字节密钥"));
        }
        if peer
            .get("keepalive_secs")
            .is_some_and(|value| !value.as_u64().is_some_and(|value| value <= 65535))
        {
            return Err(invalid("Peer 保活间隔必须为 0～65535 秒的整数"));
        }
        if !peer["allowed_ips"]
            .as_array()
            .is_some_and(|v| !v.is_empty() && v.iter().all(cidr))
        {
            return Err(invalid("Peer allowed_ips 必须为非空 CIDR 列表"));
        }
        if let Some(endpoint) = peer.get("endpoint") {
            let valid = endpoint
                .as_str()
                .and_then(|s| s.rsplit_once(':'))
                .is_some_and(|(host, port)| {
                    !host.is_empty()
                        && !host.chars().any(char::is_whitespace)
                        && (if host.starts_with('[') && host.ends_with(']') {
                            host[1..host.len() - 1]
                                .parse::<std::net::Ipv6Addr>()
                                .is_ok()
                        } else {
                            host.chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                        })
                        && port.parse::<u16>().is_ok_and(|p| p > 0)
                });
            if !valid {
                return Err(invalid(
                    "Peer endpoint 必须为主机:端口，IPv6 请使用 [地址]:端口",
                ));
            }
        }
    }
    Ok(())
}
