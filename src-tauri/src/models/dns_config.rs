use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{AppError, AppResult};

pub const CLIENT_DNS_DETOUR_ROUTE_FINAL: &str = "$route_final";

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
                if host != host.trim() {
                    return Err(AppError::invalid_argument(format!(
                        "DNS server `{name}` host must not contain surrounding whitespace"
                    )));
                }
                if *port == 0 {
                    return Err(AppError::invalid_argument(format!(
                        "DNS server `{name}` requires a non-zero port"
                    )));
                }
                if let Self::Doh { path, .. } = self {
                    if !path.starts_with('/') {
                        return Err(AppError::invalid_argument(format!(
                            "DNS server `{name}` requires a DoH path starting with `/`"
                        )));
                    }
                }
                if let Some(detour) = self.detour() {
                    if detour.trim().is_empty() {
                        return Err(AppError::invalid_argument(format!(
                            "DNS server `{name}` has an empty detour"
                        )));
                    }
                    if matches!(self, Self::Doq { .. }) {
                        return Err(AppError::invalid_argument(format!(
                            "DNS server `{name}` cannot use a DoQ detour"
                        )));
                    }
                }
                Ok(())
            }
        }
    }

    fn detour(&self) -> Option<&str> {
        match self {
            Self::System { .. } => None,
            Self::Udp { detour, .. }
            | Self::Doh { detour, .. }
            | Self::Dot { detour, .. }
            | Self::Doq { detour, .. } => detour.as_deref(),
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ipv6_cidr: Option<String>,
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
    pub fn recommended_default() -> Self {
        let mut servers = BTreeMap::new();
        servers.insert(
            "cloudflare".to_string(),
            ClientDnsServer::Doh {
                host: "cloudflare-dns.com".to_string(),
                port: 443,
                path: "/dns-query".to_string(),
                bootstrap: vec![
                    IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                    IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
                ],
                server_name: None,
                detour: Some(CLIENT_DNS_DETOUR_ROUTE_FINAL.to_string()),
                extra: BTreeMap::new(),
            },
        );
        servers.insert(
            "google".to_string(),
            ClientDnsServer::Doh {
                host: "dns.google".to_string(),
                port: 443,
                path: "/dns-query".to_string(),
                bootstrap: vec![
                    IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
                    IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)),
                ],
                server_name: None,
                detour: Some(CLIENT_DNS_DETOUR_ROUTE_FINAL.to_string()),
                extra: BTreeMap::new(),
            },
        );
        servers.insert(
            "cloudflare-bootstrap".to_string(),
            ClientDnsServer::Doh {
                host: "cloudflare-dns.com".to_string(),
                port: 443,
                path: "/dns-query".to_string(),
                bootstrap: vec![
                    IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                    IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
                ],
                server_name: None,
                detour: None,
                extra: BTreeMap::new(),
            },
        );
        servers.insert(
            "google-bootstrap".to_string(),
            ClientDnsServer::Doh {
                host: "dns.google".to_string(),
                port: 443,
                path: "/dns-query".to_string(),
                bootstrap: vec![
                    IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
                    IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)),
                ],
                server_name: None,
                detour: None,
                extra: BTreeMap::new(),
            },
        );
        servers.insert("alidns".to_string(), recommended_alidns_server());
        servers.insert("114dns".to_string(), recommended_114dns_server());
        servers.insert(
            "system".to_string(),
            ClientDnsServer::System {
                extra: BTreeMap::new(),
            },
        );

        Self {
            servers,
            default_server: "cloudflare".to_string(),
            dispatch: Vec::new(),
            cache: Some(ClientDnsCache {
                max_entries: 1024,
                max_ttl_seconds: None,
                extra: BTreeMap::new(),
            }),
            reverse_mapping: None,
            // TUN is opt-in, but once a new user enables it the recommended
            // DNS path should preserve domain identity for routing instead of
            // silently falling back to real-address interception.
            answer: ClientDnsAnswer::FakeIp {
                cidr: default_fake_ip_cidr(),
                ipv6_cidr: Some(default_fake_ip_ipv6_cidr()),
                ttl_seconds: default_fake_ip_ttl(),
                max_entries: None,
                exclude_domains: Vec::new(),
                extra: BTreeMap::new(),
            },
            policy: Some(ClientDnsPolicy {
                timeout_ms: None,
                server_timeout_ms: None,
                fallback_servers: Some(vec!["google".to_string(), "system".to_string()]),
                node_server: Some("system".to_string()),
                node_fallback_servers: Some(vec![
                    "cloudflare-bootstrap".to_string(),
                    "google-bootstrap".to_string(),
                ]),
                direct_server: None,
                direct_fallback_servers: None,
                reject_address_cidrs: None,
                address_family: Some(ClientDnsAddressFamilyPolicy::PreferIpv4),
                extra: BTreeMap::new(),
            }),
            extra: BTreeMap::new(),
        }
    }

    /// Add the client-owned domestic resolver presets without changing any
    /// dispatch or fallback order. Only configurations that still retain the
    /// original recommended resolver scaffold are migrated; imported or
    /// user-replaced server definitions remain authoritative.
    pub fn migrate_missing_builtin_domestic_resolvers(&mut self) -> bool {
        let recommended = Self::recommended_default();
        let retains_recommended_scaffold = [
            "cloudflare",
            "google",
            "cloudflare-bootstrap",
            "google-bootstrap",
            "system",
        ]
        .iter()
        .all(|tag| self.servers.get(*tag) == recommended.servers.get(*tag));
        if !retains_recommended_scaffold {
            return false;
        }

        let mut changed = false;
        if !self.servers.contains_key("alidns") {
            self.servers
                .insert("alidns".to_string(), recommended_alidns_server());
            changed = true;
        }
        if !self.servers.contains_key("114dns") {
            self.servers
                .insert("114dns".to_string(), recommended_114dns_server());
            changed = true;
        }
        changed
    }

    /// Move only the previous built-in node-resolution policy to the safer
    /// system-first order. Custom server definitions or policy orders are
    /// deliberately left untouched.
    pub fn migrate_legacy_recommended_node_resolution(&mut self) -> bool {
        let legacy = Self::legacy_recommended_default();
        let uses_builtin_servers = ["system", "cloudflare-bootstrap", "google-bootstrap"]
            .into_iter()
            .all(|tag| self.servers.get(tag) == legacy.servers.get(tag));
        let uses_legacy_policy = self.policy.as_ref().is_some_and(|policy| {
            policy.node_server.as_deref() == Some("cloudflare-bootstrap")
                && policy.node_fallback_servers.as_deref()
                    == Some(["google-bootstrap".to_string(), "system".to_string()].as_slice())
        });
        if !uses_builtin_servers || !uses_legacy_policy {
            return false;
        }

        let policy = self
            .policy
            .as_mut()
            .expect("legacy policy match requires DNS policy");
        policy.node_server = Some("system".to_string());
        policy.node_fallback_servers = Some(vec![
            "cloudflare-bootstrap".to_string(),
            "google-bootstrap".to_string(),
        ]);
        true
    }

    fn legacy_recommended_default() -> Self {
        let mut config = Self::recommended_default();
        let policy = config
            .policy
            .as_mut()
            .expect("recommended DNS configuration has a policy");
        policy.node_server = Some("cloudflare-bootstrap".to_string());
        policy.node_fallback_servers =
            Some(vec!["google-bootstrap".to_string(), "system".to_string()]);
        config
    }

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
        if let Some(policy) = &self.policy {
            let ensure_server = |field: &str, name: &str| -> AppResult<()> {
                if self.servers.contains_key(name) {
                    Ok(())
                } else {
                    Err(AppError::invalid_argument(format!(
                        "DNS policy {field} references undefined server `{name}`"
                    )))
                }
            };
            for (field, name) in [
                ("node_server", policy.node_server.as_deref()),
                ("direct_server", policy.direct_server.as_deref()),
            ] {
                if let Some(name) = name {
                    ensure_server(field, name)?;
                }
            }
            for (field, names) in [
                ("fallback_servers", policy.fallback_servers.as_deref()),
                (
                    "node_fallback_servers",
                    policy.node_fallback_servers.as_deref(),
                ),
                (
                    "direct_fallback_servers",
                    policy.direct_fallback_servers.as_deref(),
                ),
            ] {
                for name in names.unwrap_or_default() {
                    ensure_server(field, name)?;
                }
            }
            let validate_fallbacks =
                |field: &str, primary: Option<&str>, names: Option<&[String]>| -> AppResult<()> {
                    let mut seen = BTreeSet::new();
                    for name in names.unwrap_or_default() {
                        if !seen.insert(name.as_str()) {
                            return Err(AppError::invalid_argument(format!(
                                "DNS policy {field} contains duplicate server `{name}`"
                            )));
                        }
                        if primary == Some(name.as_str()) {
                            return Err(AppError::invalid_argument(format!(
                                "DNS policy {field} repeats primary server `{name}`"
                            )));
                        }
                    }
                    Ok(())
                };
            validate_fallbacks(
                "fallback_servers",
                Some(self.default_server.as_str()),
                policy.fallback_servers.as_deref(),
            )?;
            if policy.node_server.is_none()
                && policy
                    .node_fallback_servers
                    .as_ref()
                    .is_some_and(|servers| !servers.is_empty())
            {
                return Err(AppError::invalid_argument(
                    "DNS policy node_fallback_servers requires node_server",
                ));
            }
            validate_fallbacks(
                "node_fallback_servers",
                policy.node_server.as_deref(),
                policy.node_fallback_servers.as_deref(),
            )?;
            if policy.direct_server.is_none()
                && policy
                    .direct_fallback_servers
                    .as_ref()
                    .is_some_and(|servers| !servers.is_empty())
            {
                return Err(AppError::invalid_argument(
                    "DNS policy direct_fallback_servers requires direct_server",
                ));
            }
            validate_fallbacks(
                "direct_fallback_servers",
                policy.direct_server.as_deref(),
                policy.direct_fallback_servers.as_deref(),
            )?;
            if policy
                .timeout_ms
                .is_some_and(|value| !(1..=120_000).contains(&value))
            {
                return Err(AppError::invalid_argument(
                    "DNS policy timeout_ms must be between 1 and 120000",
                ));
            }
            if let Some(timeouts) = &policy.server_timeout_ms {
                for (name, timeout) in timeouts {
                    ensure_server("server_timeout_ms", name)?;
                    if !(1..=120_000).contains(timeout) {
                        return Err(AppError::invalid_argument(format!(
                            "DNS policy server_timeout_ms.{name} must be between 1 and 120000"
                        )));
                    }
                }
            }
            for cidr in policy.reject_address_cidrs.as_deref().unwrap_or_default() {
                parse_cidr(cidr, "DNS policy reject_address_cidrs")?;
            }
        }
        let has_detour = self
            .servers
            .values()
            .any(|server| server.detour().is_some());
        if has_detour {
            let policy = self.policy.as_ref().ok_or_else(|| {
                AppError::invalid_argument(
                    "DNS detours require policy.node_server to avoid recursive resolution",
                )
            })?;
            let node_server = policy.node_server.as_deref().ok_or_else(|| {
                AppError::invalid_argument(
                    "DNS detours require policy.node_server to avoid recursive resolution",
                )
            })?;
            for name in std::iter::once(node_server).chain(
                policy
                    .node_fallback_servers
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str),
            ) {
                if self
                    .servers
                    .get(name)
                    .is_some_and(|server| server.detour().is_some())
                {
                    return Err(AppError::invalid_argument(format!(
                        "DNS node server `{name}` must not use a detour"
                    )));
                }
            }
        }
        if self.cache.as_ref().is_some_and(|cache| {
            cache.max_entries == 0 || cache.max_ttl_seconds.is_some_and(|ttl| ttl == 0)
        }) {
            return Err(AppError::invalid_argument(
                "DNS cache limits must be greater than zero",
            ));
        }
        if self.reverse_mapping.as_ref().is_some_and(|mapping| {
            mapping.max_entries == 0
                || mapping.max_domains_per_address < 2
                || mapping.max_ttl_seconds == 0
        }) {
            return Err(AppError::invalid_argument(
                "DNS reverse_mapping requires positive limits and at least two domains per address",
            ));
        }
        if let ClientDnsAnswer::FakeIp {
            cidr,
            ipv6_cidr,
            ttl_seconds,
            max_entries,
            ..
        } = &self.answer
        {
            let (address, prefix) = parse_cidr(cidr, "Fake-IP CIDR")?;
            if !address.is_ipv4() {
                return Err(AppError::invalid_argument(
                    "Fake-IP CIDR must be an IPv4 CIDR",
                ));
            }
            if prefix > 30 {
                return Err(AppError::invalid_argument(
                    "Fake-IP CIDR must provide at least four addresses (maximum prefix /30)",
                ));
            }
            let usable_ipv4 = usable_ipv4_addresses(prefix);
            let usable_ipv6 = ipv6_cidr
                .as_deref()
                .map(|cidr| {
                    let (address, prefix) = parse_cidr(cidr, "Fake-IPv6 CIDR")?;
                    if !address.is_ipv6() {
                        return Err(AppError::invalid_argument(
                            "Fake-IPv6 CIDR must be an IPv6 CIDR",
                        ));
                    }
                    if prefix > 126 {
                        return Err(AppError::invalid_argument(
                            "Fake-IPv6 CIDR must provide at least four addresses (maximum prefix /126)",
                        ));
                    }
                    Ok(usable_ipv6_addresses(prefix))
                })
                .transpose()?;
            let usable_addresses = usable_ipv6.map_or(usable_ipv4, |value| value.min(usable_ipv4));
            if *ttl_seconds == 0
                || max_entries.is_some_and(|entries| entries == 0 || entries > usable_addresses)
            {
                return Err(AppError::invalid_argument(
                    "Fake-IP TTL must be positive and max entries must fit the configured address pools",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_tun_owned_addresses(&self, addresses: &[IpAddr]) -> AppResult<()> {
        let ClientDnsAnswer::FakeIp {
            cidr, ipv6_cidr, ..
        } = &self.answer
        else {
            return Ok(());
        };
        let ipv4 = parse_cidr(cidr, "Fake-IP CIDR")?;
        let ipv6 = ipv6_cidr
            .as_deref()
            .map(|cidr| parse_cidr(cidr, "Fake-IPv6 CIDR"))
            .transpose()?;
        if let Some(address) = addresses.iter().find(|address| {
            cidr_contains(ipv4, **address)
                || ipv6.is_some_and(|network| cidr_contains(network, **address))
        }) {
            return Err(AppError::invalid_argument(format!(
                "Fake-IP pool overlaps TUN-owned address `{address}`"
            )));
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

fn recommended_alidns_server() -> ClientDnsServer {
    ClientDnsServer::Doh {
        host: "dns.alidns.com".to_string(),
        port: 443,
        path: "/dns-query".to_string(),
        bootstrap: vec![
            IpAddr::V4(Ipv4Addr::new(223, 5, 5, 5)),
            IpAddr::V4(Ipv4Addr::new(223, 6, 6, 6)),
        ],
        server_name: None,
        // Domestic CDN selection must observe the user's physical egress,
        // rather than whichever proxy node is selected as route.final.
        detour: None,
        extra: BTreeMap::new(),
    }
}

fn recommended_114dns_server() -> ClientDnsServer {
    ClientDnsServer::Udp {
        host: "114.114.114.114".to_string(),
        port: 53,
        bootstrap: Vec::new(),
        detour: None,
        extra: BTreeMap::new(),
    }
}

fn parse_cidr(value: &str, field: &str) -> AppResult<(IpAddr, u8)> {
    let (address, prefix) = value
        .trim()
        .split_once('/')
        .ok_or_else(|| AppError::invalid_argument(format!("{field} must use CIDR notation")))?;
    let address = address.parse::<IpAddr>().map_err(|_| {
        AppError::invalid_argument(format!("{field} contains an invalid IP address"))
    })?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| AppError::invalid_argument(format!("{field} contains an invalid prefix")))?;
    let maximum = if address.is_ipv4() { 32 } else { 128 };
    if prefix > maximum {
        return Err(AppError::invalid_argument(format!(
            "{field} prefix must be between 0 and {maximum}"
        )));
    }
    Ok((address, prefix))
}

fn usable_ipv4_addresses(prefix: u8) -> usize {
    ((1_u128 << (32 - prefix)) - 2).min(usize::MAX as u128) as usize
}

fn usable_ipv6_addresses(prefix: u8) -> usize {
    let host_bits = 128 - prefix;
    if u32::from(host_bits) >= usize::BITS {
        usize::MAX
    } else {
        1_usize << host_bits
    }
}

fn cidr_contains((network, prefix): (IpAddr, u8), address: IpAddr) -> bool {
    match (network, address) {
        (IpAddr::V4(network), IpAddr::V4(address)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            u32::from(network) & mask == u32::from(address) & mask
        }
        (IpAddr::V6(network), IpAddr::V6(address)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            u128::from(network) & mask == u128::from(address) & mask
        }
        _ => false,
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
fn default_fake_ip_ipv6_cidr() -> String {
    "fd00::/96".to_owned()
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
            "answer": {
                "type": "fake_ip",
                "cidr": "198.18.0.0/15",
                "ipv6_cidr": "fd00::/96",
                "ttl_seconds": 86400
            },
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

    #[test]
    fn dns_contract_rejects_invalid_fake_ipv6_cidr() {
        let model: ClientDnsConfig = serde_json::from_value(json!({
            "servers": { "system": { "type": "system" } },
            "default_server": "system",
            "answer": {
                "type": "fake_ip",
                "cidr": "198.18.0.0/15",
                "ipv6_cidr": "not-an-ipv6-cidr"
            }
        }))
        .unwrap();
        assert!(model.validate_client_shape().is_err());
    }

    #[test]
    fn dns_contract_rejects_recursive_or_unsupported_detours_before_runtime() {
        for value in [
            json!({
                "servers": {
                    "proxy": {
                        "type": "doh", "host": "dns.example", "path": "/dns-query",
                        "bootstrap": ["1.1.1.1"], "detour": "$route_final"
                    }
                },
                "default_server": "proxy"
            }),
            json!({
                "servers": {
                    "proxy": {
                        "type": "doh", "host": "dns.example", "path": "/dns-query",
                        "bootstrap": ["1.1.1.1"], "detour": "$route_final"
                    }
                },
                "default_server": "proxy",
                "policy": { "node_server": "proxy" }
            }),
            json!({
                "servers": {
                    "bootstrap": { "type": "system" },
                    "proxy": {
                        "type": "doq", "host": "dns.example", "bootstrap": ["1.1.1.1"],
                        "detour": "$route_final"
                    }
                },
                "default_server": "proxy",
                "policy": { "node_server": "bootstrap" }
            }),
        ] {
            let model: ClientDnsConfig = serde_json::from_value(value).unwrap();
            assert!(model.validate_client_shape().is_err());
        }
    }

    #[test]
    fn dns_contract_rejects_invalid_fallbacks_and_timeouts_before_runtime() {
        for policy in [
            json!({ "fallback_servers": ["secondary", "secondary"] }),
            json!({ "fallback_servers": ["primary"] }),
            json!({ "node_fallback_servers": ["secondary"] }),
            json!({ "direct_server": "primary", "direct_fallback_servers": ["primary"] }),
            json!({ "timeout_ms": 0 }),
            json!({ "server_timeout_ms": { "primary": 120001 } }),
        ] {
            let model: ClientDnsConfig = serde_json::from_value(json!({
                "servers": {
                    "primary": { "type": "system" },
                    "secondary": { "type": "udp", "host": "1.1.1.1" }
                },
                "default_server": "primary",
                "policy": policy
            }))
            .unwrap();
            assert!(model.validate_client_shape().is_err());
        }
    }

    #[test]
    fn dns_contract_rejects_invalid_policy_and_fake_ip_cidrs() {
        let invalid_policy: ClientDnsConfig = serde_json::from_value(json!({
            "servers": { "system": { "type": "system" } },
            "default_server": "system",
            "policy": { "reject_address_cidrs": ["not-a-cidr"] }
        }))
        .unwrap();
        assert!(invalid_policy.validate_client_shape().is_err());

        for answer in [
            json!({ "type": "fake_ip", "cidr": "fd00::/96" }),
            json!({ "type": "fake_ip", "cidr": "198.18.0.0/31" }),
            json!({ "type": "fake_ip", "cidr": "198.18.0.0/30", "max_entries": 3 }),
            json!({
                "type": "fake_ip",
                "cidr": "198.18.0.0/15",
                "ipv6_cidr": "198.19.0.0/16"
            }),
        ] {
            let model: ClientDnsConfig = serde_json::from_value(json!({
                "servers": { "system": { "type": "system" } },
                "default_server": "system",
                "answer": answer
            }))
            .unwrap();
            assert!(model.validate_client_shape().is_err());
        }
    }

    #[test]
    fn fake_ip_pool_rejects_tun_owned_addresses() {
        let model: ClientDnsConfig = serde_json::from_value(json!({
            "servers": { "system": { "type": "system" } },
            "default_server": "system",
            "answer": { "type": "fake_ip", "cidr": "10.66.0.0/24" }
        }))
        .unwrap();

        assert!(model
            .validate_tun_owned_addresses(&["10.66.0.1".parse().unwrap()])
            .is_err());
        assert!(model
            .validate_tun_owned_addresses(&["10.67.0.1".parse().unwrap()])
            .is_ok());
    }

    #[test]
    fn dns_contract_rejects_zero_cache_and_reverse_mapping_limits() {
        for extra in [
            json!({ "cache": { "max_entries": 0 } }),
            json!({
                "reverse_mapping": {
                    "max_entries": 1,
                    "max_domains_per_address": 1,
                    "max_ttl_seconds": 300
                }
            }),
        ] {
            let mut value = json!({
                "servers": { "system": { "type": "system" } },
                "default_server": "system"
            });
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let model: ClientDnsConfig = serde_json::from_value(value).unwrap();
            assert!(model.validate_client_shape().is_err());
        }
    }

    #[test]
    fn recommended_default_uses_public_dns_with_system_as_final_fallback() {
        let model = ClientDnsConfig::recommended_default();
        model.validate_client_shape().unwrap();
        let value = serde_json::to_value(model).unwrap();

        assert_eq!(value["default_server"], "cloudflare");
        assert_eq!(
            value["servers"]["cloudflare"]["detour"],
            CLIENT_DNS_DETOUR_ROUTE_FINAL
        );
        assert_eq!(
            value["servers"]["google"]["detour"],
            CLIENT_DNS_DETOUR_ROUTE_FINAL
        );
        assert_eq!(
            value["policy"]["fallback_servers"],
            json!(["google", "system"])
        );
        assert_eq!(value["policy"]["node_server"], "system");
        assert_eq!(
            value["policy"]["node_fallback_servers"],
            json!(["cloudflare-bootstrap", "google-bootstrap"])
        );
        assert!(value["servers"]["cloudflare-bootstrap"]
            .get("detour")
            .is_none());
        assert!(value["servers"]["google-bootstrap"].get("detour").is_none());
        assert_eq!(value["servers"]["alidns"]["type"], "doh");
        assert_eq!(value["servers"]["alidns"]["host"], "dns.alidns.com");
        assert_eq!(
            value["servers"]["alidns"]["bootstrap"],
            json!(["223.5.5.5", "223.6.6.6"])
        );
        assert!(value["servers"]["alidns"].get("detour").is_none());
        assert_eq!(value["servers"]["114dns"]["type"], "udp");
        assert_eq!(value["servers"]["114dns"]["host"], "114.114.114.114");
        assert!(value["servers"]["114dns"].get("detour").is_none());
        assert_eq!(value["answer"]["type"], "fake_ip");
        assert_eq!(value["answer"]["cidr"], "198.18.0.0/15");
        assert_eq!(value["answer"]["ipv6_cidr"], "fd00::/96");
    }

    #[test]
    fn missing_answer_in_an_existing_dns_config_keeps_real_ip_compatibility() {
        let mut value = serde_json::to_value(ClientDnsConfig::recommended_default()).unwrap();
        value.as_object_mut().unwrap().remove("answer");

        let restored: ClientDnsConfig = serde_json::from_value(value).unwrap();

        assert_eq!(restored.answer, ClientDnsAnswer::Real);
    }

    #[test]
    fn domestic_resolver_migration_is_additive_and_preserves_custom_servers() {
        let mut previous = ClientDnsConfig::recommended_default();
        previous.servers.remove("alidns");
        previous.servers.remove("114dns");

        assert!(previous.migrate_missing_builtin_domestic_resolvers());
        assert!(previous.servers.contains_key("alidns"));
        assert!(previous.servers.contains_key("114dns"));
        assert!(!previous.migrate_missing_builtin_domestic_resolvers());

        let mut custom = previous.clone();
        custom.servers.remove("alidns");
        custom.servers.insert(
            "cloudflare".to_string(),
            ClientDnsServer::System {
                extra: BTreeMap::new(),
            },
        );
        assert!(!custom.migrate_missing_builtin_domestic_resolvers());
        assert!(!custom.servers.contains_key("alidns"));
    }

    #[test]
    fn legacy_recommended_node_resolution_migrates_without_overwriting_custom_policy() {
        let mut legacy = ClientDnsConfig::legacy_recommended_default();
        assert!(legacy.migrate_legacy_recommended_node_resolution());
        let policy = legacy.policy.as_ref().unwrap();
        assert_eq!(policy.node_server.as_deref(), Some("system"));
        assert_eq!(
            policy.node_fallback_servers.as_deref(),
            Some(
                [
                    "cloudflare-bootstrap".to_string(),
                    "google-bootstrap".to_string()
                ]
                .as_slice()
            )
        );
        assert!(!legacy.migrate_legacy_recommended_node_resolution());

        let mut custom = ClientDnsConfig::legacy_recommended_default();
        custom.policy.as_mut().unwrap().node_fallback_servers = Some(vec!["system".to_string()]);
        assert!(!custom.migrate_legacy_recommended_node_resolution());
        assert_eq!(
            custom.policy.unwrap().node_fallback_servers,
            Some(vec!["system".to_string()])
        );

        let mut custom_server = ClientDnsConfig::legacy_recommended_default();
        custom_server.servers.insert(
            "cloudflare-bootstrap".to_string(),
            ClientDnsServer::System {
                extra: BTreeMap::new(),
            },
        );
        assert!(!custom_server.migrate_legacy_recommended_node_resolution());
    }
}
