use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{AppError, AppResult};

/// Lossless client-side representation of Zero's runtime DNS contract.
/// Additive fields from a newer kernel are retained instead of silently lost.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientDnsConfig {
    pub servers: BTreeMap<String, ClientDnsServer>,
    pub default_server: String,
    #[serde(default)]
    pub dispatch: Vec<ClientDnsDispatch>,
    #[serde(default)]
    pub cache: Option<ClientDnsCache>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse_mapping: Option<ClientDnsReverseMapping>,
    #[serde(default)]
    pub answer: ClientDnsAnswer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ClientDnsPolicy>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientDnsPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_timeout_ms: Option<BTreeMap<String, u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_servers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_fallback_servers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_fallback_servers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_address_cidrs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_family: Option<ClientDnsAddressFamilyPolicy>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientDnsAddressFamilyPolicy {
    Ipv4Only,
    Ipv6Only,
    #[default]
    PreferIpv4,
    PreferIpv6,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientDnsServer {
    #[serde(rename = "system")]
    System {
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
    #[serde(rename = "udp")]
    Udp {
        host: String,
        #[serde(default = "default_dns_port")]
        port: u16,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bootstrap: Vec<IpAddr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detour: Option<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
    #[serde(rename = "doh")]
    Doh {
        host: String,
        #[serde(default = "default_tls_port")]
        port: u16,
        #[serde(default = "default_doh_path")]
        path: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bootstrap: Vec<IpAddr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detour: Option<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
    #[serde(rename = "dot")]
    Dot {
        host: String,
        #[serde(default = "default_dot_port")]
        port: u16,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bootstrap: Vec<IpAddr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detour: Option<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
    #[serde(rename = "doq")]
    Doq {
        host: String,
        #[serde(default = "default_dot_port")]
        port: u16,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bootstrap: Vec<IpAddr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detour: Option<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
}

impl ClientDnsServer {
    fn validate(&self, name: &str) -> AppResult<()> {
        match self {
            Self::System { .. } => Ok(()),
            Self::Udp { host, port, .. }
            | Self::Doh { host, port, .. }
            | Self::Dot { host, port, .. }
            | Self::Doq { host, port, .. } => {
                if host.trim().is_empty() {
                    return Err(AppError::invalid_argument(format!(
                        "DNS server `{name}` requires a host"
                    )));
                }
                if *port == 0 {
                    return Err(AppError::invalid_argument(format!(
                        "DNS server `{name}` requires a non-zero port"
                    )));
                }
                Ok(())
            }
        }
    }

    pub fn requires_bootstrap(&self) -> bool {
        match self {
            Self::System { .. } => false,
            Self::Udp {
                host, bootstrap, ..
            }
            | Self::Doh {
                host, bootstrap, ..
            }
            | Self::Dot {
                host, bootstrap, ..
            }
            | Self::Doq {
                host, bootstrap, ..
            } => host.parse::<IpAddr>().is_err() && bootstrap.is_empty(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientDnsDispatch {
    /// The shared Zero routing condition; the client does not implement a
    /// second matcher and preserves nested/future condition variants.
    pub condition: Value,
    pub server: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientDnsCache {
    #[serde(default = "default_cache_entries")]
    pub max_entries: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ttl_seconds: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientDnsReverseMapping {
    #[serde(default = "default_reverse_mapping_entries")]
    pub max_entries: usize,
    #[serde(default = "default_reverse_mapping_domains_per_address")]
    pub max_domains_per_address: usize,
    #[serde(default = "default_reverse_mapping_ttl")]
    pub max_ttl_seconds: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum ClientDnsAnswer {
    #[default]
    #[serde(rename = "real")]
    Real,
    #[serde(rename = "fake_ip")]
    FakeIp {
        #[serde(default = "default_fake_ip_cidr")]
        cidr: String,
        #[serde(default = "default_fake_ip_ttl")]
        ttl_seconds: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_entries: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_domains: Vec<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
}

impl ClientDnsConfig {
    pub fn validate_client_shape(&self) -> AppResult<()> {
        if self.servers.is_empty() {
            return Err(AppError::invalid_argument(
                "DNS requires at least one named server",
            ));
        }
        if !self.servers.contains_key(&self.default_server) {
            return Err(AppError::invalid_argument(format!(
                "DNS default server `{}` does not exist",
                self.default_server
            )));
        }
        for (name, server) in &self.servers {
            if name.trim().is_empty() {
                return Err(AppError::invalid_argument(
                    "DNS server names cannot be empty",
                ));
            }
            server.validate(name)?;
        }
        for (index, rule) in self.dispatch.iter().enumerate() {
            if !self.servers.contains_key(&rule.server) {
                return Err(AppError::invalid_argument(format!(
                    "DNS dispatch {index} references undefined server `{}`",
                    rule.server
                )));
            }
            if !rule.condition.is_object() {
                return Err(AppError::invalid_argument(format!(
                    "DNS dispatch {index} condition must be an object"
                )));
            }
        }
        if let ClientDnsAnswer::FakeIp {
            cidr,
            ttl_seconds,
            max_entries,
            ..
        } = &self.answer
        {
            if !cidr.contains('/') {
                return Err(AppError::invalid_argument(
                    "Fake-IP CIDR must use CIDR notation",
                ));
            }
            if *ttl_seconds == 0 || max_entries.is_some_and(|entries| entries == 0) {
                return Err(AppError::invalid_argument(
                    "Fake-IP TTL and max entries must be greater than zero",
                ));
            }
        }
        Ok(())
    }

    pub fn bootstrap_warnings(&self) -> Vec<String> {
        self.servers
            .iter()
            .filter(|(_, server)| server.requires_bootstrap())
            .map(|(name, _)| {
                format!("DNS server `{name}` uses a domain endpoint without bootstrap addresses")
            })
            .collect()
    }

    pub fn rename_server(&mut self, old_name: &str, new_name: String) -> AppResult<()> {
        let new_name = new_name.trim().to_owned();
        if new_name.is_empty() || self.servers.contains_key(&new_name) {
            return Err(AppError::invalid_argument(
                "DNS server name must be non-empty and unique",
            ));
        }
        let server = self
            .servers
            .remove(old_name)
            .ok_or_else(|| AppError::not_found("dns_server", old_name))?;
        self.servers.insert(new_name.clone(), server);
        if self.default_server == old_name {
            self.default_server.clone_from(&new_name);
        }
        for rule in &mut self.dispatch {
            if rule.server == old_name {
                rule.server.clone_from(&new_name);
            }
        }
        Ok(())
    }
}

const fn default_dns_port() -> u16 {
    53
}
const fn default_tls_port() -> u16 {
    443
}
const fn default_dot_port() -> u16 {
    853
}
const fn default_cache_entries() -> usize {
    256
}
const fn default_reverse_mapping_entries() -> usize {
    1024
}
const fn default_reverse_mapping_domains_per_address() -> usize {
    8
}
const fn default_reverse_mapping_ttl() -> u64 {
    300
}
const fn default_fake_ip_ttl() -> u64 {
    86_400
}
fn default_doh_path() -> String {
    "/dns-query".to_owned()
}
fn default_fake_ip_cidr() -> String {
    "198.18.0.0/15".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dns_contract_round_trips_unknown_fields_and_rule_order() {
        let source = json!({
            "servers": {
                "global": {
                    "type": "doh", "host": "cloudflare-dns.com", "port": 443,
                    "path": "/dns-query", "bootstrap": ["1.1.1.1"],
                    "detour": "proxy",
                    "future_transport_option": true
                },
                "system": { "type": "system" }
            },
            "default_server": "global",
            "dispatch": [
                { "condition": { "domain": ["open.bigmodel.cn"] }, "server": "global" },
                { "condition": { "domain_keyword": ["internal"] }, "server": "system" }
            ],
            "cache": { "max_entries": 1024, "future_cache_option": "keep" },
            "reverse_mapping": {
                "max_entries": 1024,
                "max_domains_per_address": 8,
                "max_ttl_seconds": 300,
                "future_reverse_option": true
            },
            "answer": { "type": "fake_ip", "cidr": "198.18.0.0/15", "ttl_seconds": 86400 },
            "policy": {
                "timeout_ms": 4000,
                "server_timeout_ms": { "global": 6000 },
                "fallback_servers": ["system"],
                "node_server": "system",
                "node_fallback_servers": [],
                "direct_server": "global",
                "direct_fallback_servers": ["system"],
                "reject_address_cidrs": ["0.0.0.0/32"],
                "address_family": "prefer_ipv6",
                "future_policy_option": true
            },
            "future_dns_option": { "keep": true }
        });
        let model: ClientDnsConfig = serde_json::from_value(source.clone()).unwrap();
        model.validate_client_shape().unwrap();
        assert_eq!(serde_json::to_value(model).unwrap(), source);
    }

    #[test]
    fn renaming_server_updates_default_and_dispatch_references() {
        let mut model: ClientDnsConfig = serde_json::from_value(json!({
            "servers": { "old": { "type": "udp", "host": "1.1.1.1" } },
            "default_server": "old",
            "dispatch": [{ "condition": { "domain": ["example.com"] }, "server": "old" }]
        }))
        .unwrap();
        model.rename_server("old", "global".to_owned()).unwrap();
        assert_eq!(model.default_server, "global");
        assert_eq!(model.dispatch[0].server, "global");
        assert!(model.servers.contains_key("global"));
    }

    #[test]
    fn domain_endpoint_without_bootstrap_is_a_visible_warning() {
        let model: ClientDnsConfig = serde_json::from_value(json!({
            "servers": { "global": { "type": "doq", "host": "dns.example" } },
            "default_server": "global"
        }))
        .unwrap();
        assert_eq!(model.bootstrap_warnings().len(), 1);
    }

    #[test]
    fn dns_contract_rejects_unknown_address_family_policy() {
        let result = serde_json::from_value::<ClientDnsConfig>(json!({
            "servers": { "system": { "type": "system" } },
            "default_server": "system",
            "policy": { "address_family": "automatic_magic" }
        }));
        assert!(result.is_err());
    }
}
