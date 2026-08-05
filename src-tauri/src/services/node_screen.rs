use std::collections::BTreeMap;

use crate::client_core::{
    NodeGroupSnapshot, NodeObservationSource, NodeScreenSnapshot, NodeSnapshot, ProbeJobState,
    ProbeObservationSource, SourceStatus,
};
use crate::errors::AppResult;
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::gui_core::{ConfigProxyNode, GuiPolicyGroup};
use crate::services::{common, core_config};
use crate::state::app_state::AppState;

pub async fn snapshot(state: &AppState) -> AppResult<NodeScreenSnapshot> {
    let client = state.client_core_snapshot();
    let Some(profile_id) = client.scope.profile_id.clone() else {
        return Ok(NodeScreenSnapshot {
            revision: client.revision,
            scope: client.scope,
            source_status: client.source_status,
            groups: Vec::new(),
            nodes: Vec::new(),
            active_probe_jobs: client.active_probe_jobs,
        });
    };

    let active_content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone());
    let adapter = ZeroAdapter::new();
    let config_nodes = active_content
        .as_ref()
        .map(|content| adapter.proxy_nodes_from_config(content))
        .transpose()?
        .unwrap_or_default();
    let config_groups = active_content
        .as_ref()
        .map(|content| adapter.policy_groups_from_config(content))
        .transpose()?
        .unwrap_or_default();

    let options = {
        let config = common::lock(state.app_config(), "app_config")?;
        core_config::ipc_options_from_app_config(&config.core)
    };
    let (runtime_groups, runtime_available) = match adapter.policy_groups(options).await {
        Ok(groups) => (groups, true),
        Err(_) => (Vec::new(), false),
    };

    let mut nodes = BTreeMap::<String, NodeSnapshot>::new();
    for node in config_nodes {
        nodes.insert(
            node.tag.clone(),
            node_from_config(&client.scope, node, runtime_available),
        );
    }

    let mut groups = BTreeMap::<String, NodeGroupSnapshot>::new();
    for group in config_groups {
        merge_group(&client.scope, &mut groups, &mut nodes, group, false);
    }
    for group in runtime_groups {
        merge_group(&client.scope, &mut groups, &mut nodes, group, true);
    }

    for job in state.list_client_probe_jobs(Some(profile_id.0.clone())) {
        if job.scope != client.scope {
            continue;
        }
        if job.state == ProbeJobState::Running {
            for target in &job.target_tags {
                if let Some(node) = nodes.get_mut(target) {
                    node.active_probe_job_ids.push(job.id);
                }
            }
        }
    }
    for observation in state.client_probe_observations_for_config(&client.scope) {
        let Some(node) = nodes.get_mut(&observation.target_tag) else {
            continue;
        };
        node.history.push(observation.clone());
        if node
            .last_observed_at_unix_ms
            .is_some_and(|current| current > observation.observed_at_unix_ms)
        {
            continue;
        }
        node.alive = Some(observation.reachable);
        node.latency_ms = observation.latency_ms;
        node.last_observed_at_unix_ms = Some(observation.observed_at_unix_ms);
        node.last_observation_source = Some(match observation.source {
            ProbeObservationSource::ManualOutbound => NodeObservationSource::ManualOutbound,
            ProbeObservationSource::ManualPolicy => NodeObservationSource::ManualPolicy,
            ProbeObservationSource::ScheduledPolicy => NodeObservationSource::ScheduledPolicy,
        });
    }

    let source_status = if runtime_available {
        client.source_status
    } else if client.source_status == SourceStatus::Offline {
        SourceStatus::Offline
    } else {
        SourceStatus::Degraded
    };
    for node in nodes.values_mut() {
        node.active_probe_job_ids.sort_unstable();
        node.active_probe_job_ids.dedup();
        node.action_valid = source_status == SourceStatus::Ready && action_valid(&node.protocol);
    }

    Ok(NodeScreenSnapshot {
        revision: client.revision,
        scope: client.scope,
        source_status,
        groups: groups.into_values().collect(),
        nodes: nodes.into_values().collect(),
        active_probe_jobs: client.active_probe_jobs,
    })
}

fn node_from_config(
    scope: &crate::client_core::ClientScope,
    node: ConfigProxyNode,
    runtime_available: bool,
) -> NodeSnapshot {
    NodeSnapshot {
        id: scope
            .node_id(node.tag.clone())
            .expect("active profile checked before node projection"),
        tag: node.tag,
        protocol: node.protocol,
        server: node.server,
        port: node.port.map(u64::from),
        udp: node.udp,
        network: node.network,
        tls: node.tls,
        sni: node.sni,
        cipher: node.cipher,
        group_tags: Vec::new(),
        selected_in: Vec::new(),
        runtime_available,
        alive: None,
        latency_ms: None,
        last_observed_at_unix_ms: None,
        last_observation_source: None,
        active_probe_job_ids: Vec::new(),
        history: Vec::new(),
        action_valid: false,
    }
}

fn merge_group(
    scope: &crate::client_core::ClientScope,
    groups: &mut BTreeMap<String, NodeGroupSnapshot>,
    nodes: &mut BTreeMap<String, NodeSnapshot>,
    group: GuiPolicyGroup,
    runtime: bool,
) {
    let member_tags: Vec<_> = group
        .outbounds
        .iter()
        .map(|member| member.tag.clone())
        .collect();
    let entry = groups
        .entry(group.name.clone())
        .or_insert_with(|| NodeGroupSnapshot {
            id: scope
                .policy_id(group.name.clone())
                .expect("active profile checked before group projection"),
            tag: group.name.clone(),
            kind: group.kind.clone(),
            selected: group.selected.clone(),
            member_tags: member_tags.clone(),
            runtime_available: runtime,
            available: group.available,
            reason: group.reason.clone(),
        });
    if runtime {
        entry.kind = group.kind.clone();
        entry.selected = group.selected.clone();
        entry.member_tags = member_tags;
        entry.runtime_available = true;
        entry.available = group.available;
        entry.reason = group.reason.clone();
    }

    for member in group.outbounds {
        let node = nodes
            .entry(member.tag.clone())
            .or_insert_with(|| NodeSnapshot {
                id: scope
                    .node_id(member.tag.clone())
                    .expect("active profile checked before member projection"),
                tag: member.tag.clone(),
                protocol: member.kind.clone().unwrap_or_else(|| "unknown".to_string()),
                server: None,
                port: None,
                udp: None,
                network: None,
                tls: None,
                sni: None,
                cipher: None,
                group_tags: Vec::new(),
                selected_in: Vec::new(),
                runtime_available: runtime,
                alive: None,
                latency_ms: None,
                last_observed_at_unix_ms: None,
                last_observation_source: None,
                active_probe_job_ids: Vec::new(),
                history: Vec::new(),
                action_valid: false,
            });
        if !node.group_tags.contains(&group.name) {
            node.group_tags.push(group.name.clone());
        }
        if group.selected.as_deref() == Some(member.tag.as_str())
            && !node.selected_in.contains(&group.name)
        {
            node.selected_in.push(group.name.clone());
        }
        if runtime {
            node.runtime_available = true;
            if !member.kind.as_deref().unwrap_or_default().is_empty() && node.protocol == "unknown"
            {
                node.protocol = member.kind.clone().unwrap_or_default();
            }
            let use_observation = match (node.last_observed_at_unix_ms, member.last_checked_unix_ms)
            {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(current), Some(next)) => next >= current,
            };
            if use_observation {
                node.alive = member.alive;
                node.latency_ms = member.delay_ms;
                node.last_observed_at_unix_ms = member.last_checked_unix_ms;
                node.last_observation_source = member
                    .last_checked_unix_ms
                    .map(|_| NodeObservationSource::RuntimeSnapshot);
            }
        }
    }
}

fn action_valid(protocol: &str) -> bool {
    !matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "direct" | "block" | "reject" | "dns" | "selector" | "fallback" | "load_balance"
    )
}

#[cfg(test)]
mod tests {
    use super::{action_valid, merge_group, node_from_config};
    use crate::client_core::{ClientScope, ConfigRevision, CoreInstanceId, ProfileId};
    use crate::models::gui_core::{ConfigProxyNode, GuiPolicyGroup, GuiPolicyMember};
    use std::collections::BTreeMap;

    fn scope() -> ClientScope {
        ClientScope {
            profile_id: Some(ProfileId("profile-a".to_string())),
            config_revision: ConfigRevision(10),
            core_instance_id: CoreInstanceId(2),
        }
    }

    fn member(tag: &str, selected: bool, delay_ms: Option<u64>) -> GuiPolicyMember {
        GuiPolicyMember {
            tag: tag.to_string(),
            kind: Some("shadowsocks".to_string()),
            selected,
            alive: delay_ms.map(|_| true),
            delay_ms,
            last_checked_unix_ms: delay_ms.map(|_| 1_000),
            last_error: None,
        }
    }

    #[test]
    fn config_and_runtime_groups_merge_into_one_authoritative_projection() {
        let scope = scope();
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "shared".to_string(),
            node_from_config(
                &scope,
                ConfigProxyNode {
                    tag: "shared".to_string(),
                    protocol: "shadowsocks".to_string(),
                    is_selector: false,
                    server: Some("example.test".to_string()),
                    port: Some(443),
                    udp: Some(true),
                    network: Some("tcp".to_string()),
                    tls: Some(true),
                    sni: Some("example.test".to_string()),
                    cipher: Some("aes-256-gcm".to_string()),
                },
                false,
            ),
        );
        let mut groups = BTreeMap::new();
        merge_group(
            &scope,
            &mut groups,
            &mut nodes,
            GuiPolicyGroup {
                name: "proxy".to_string(),
                kind: "selector".to_string(),
                selected: None,
                outbounds: vec![member("shared", false, None)],
                available: true,
                reason: None,
            },
            false,
        );
        merge_group(
            &scope,
            &mut groups,
            &mut nodes,
            GuiPolicyGroup {
                name: "proxy".to_string(),
                kind: "selector".to_string(),
                selected: Some("shared".to_string()),
                outbounds: vec![member("shared", true, Some(37))],
                available: true,
                reason: None,
            },
            true,
        );

        assert_eq!(groups.len(), 1);
        let group = groups.get("proxy").unwrap();
        assert_eq!(group.selected.as_deref(), Some("shared"));
        assert!(group.runtime_available);
        let node = nodes.get("shared").unwrap();
        assert_eq!(node.id.profile_id.0, "profile-a");
        assert_eq!(node.group_tags, vec!["proxy"]);
        assert_eq!(node.selected_in, vec!["proxy"]);
        assert_eq!(node.latency_ms, Some(37));
        assert_eq!(node.server.as_deref(), Some("example.test"));
        assert!(node.runtime_available);
    }

    #[test]
    fn action_validity_rejects_policy_and_special_outbounds() {
        assert!(action_valid("shadowsocks"));
        assert!(!action_valid("selector"));
        assert!(!action_valid("DIRECT"));
        assert!(!action_valid("block"));
    }
}
