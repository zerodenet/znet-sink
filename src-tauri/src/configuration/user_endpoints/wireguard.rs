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
pub(super) fn validate(protocol: &Value) -> AppResult<()> {
    if !key(&protocol["private_key"]) {
        return Err(invalid("WireGuard 私钥必须为 Base64 编码的 32 字节密钥"));
    }
    if !protocol["addresses"]
        .as_array()
        .is_some_and(|v| !v.is_empty() && v.iter().all(cidr))
    {
        return Err(invalid(
            "WireGuard 本机地址必须为非空的 IPv4/IPv6 CIDR 列表",
        ));
    }
    let peers = protocol["peers"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("WireGuard 至少需要一个 peer"))?;
    let mut public_keys = HashSet::new();
    for peer in peers {
        if !key(&peer["public_key"]) {
            return Err(invalid("Peer 公钥必须为 Base64 编码的 32 字节密钥"));
        }
        if !public_keys.insert(peer["public_key"].as_str().unwrap()) {
            return Err(invalid("Peer 公钥不能重复"));
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
