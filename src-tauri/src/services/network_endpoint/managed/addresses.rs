//! Public local tunnel addresses from the client's selected profile, not peers.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn configured(source: &Value, endpoints: &[Value]) -> BTreeMap<String, Vec<String>> {
    endpoints
        .iter()
        .filter_map(|endpoint| {
            let id = endpoint["endpoint_id"].as_str()?;
            // Only the known protocol configuration contract declares these addresses.
            if endpoint["protocol"] != "wireguard" {
                return None;
            }
            let legacy = endpoint
                .pointer("/configuration/origin")
                .and_then(Value::as_str)
                == Some("legacy");
            let containers: &[&str] = if legacy {
                &["inbounds", "outbounds"]
            } else {
                &["endpoints"]
            };
            let matches: Vec<_> = containers
                .iter()
                .flat_map(|key| source[*key].as_array().into_iter().flatten())
                .filter(|row| {
                    row.pointer("/protocol/type") == endpoint.get("protocol")
                        && if legacy {
                            ["inbound_tags", "outbound_tags"].iter().any(|key| {
                                endpoint[*key]
                                    .as_array()
                                    .is_some_and(|tags| tags.contains(&row["tag"]))
                            })
                        } else {
                            row["tag"] == endpoint["tag"]
                        }
                })
                .collect();
            // Ambiguous/missing declarations must not borrow another resource's IP.
            if matches.len() != 1 {
                return None;
            }
            let mut seen = BTreeSet::new();
            let addresses: Vec<_> = matches[0]
                .pointer("/protocol/addresses")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|address| !address.is_empty())
                .filter(|address| seen.insert(address.to_string()))
                .map(str::to_string)
                .collect();
            (!addresses.is_empty()).then(|| (id.to_string(), addresses))
        })
        .collect()
}
