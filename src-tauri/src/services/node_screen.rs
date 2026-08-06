use std::collections::HashMap;
use std::time::Instant;

use crate::client_core::{
    NodeGroupSnapshot, NodeObservationSource, NodeScreenSnapshot, NodeSnapshot, ProbeJobState,
    ProbeObservationSource, SourceStatus,
};
use crate::errors::AppResult;
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::gui_core::{ConfigProxyNode, GuiPolicyGroup, GuiPolicyMember};
use crate::models::logs::LogLevel;
use crate::services::{common, core_config, logs};
use crate::state::app_state::AppState;

pub async fn snapshot(state: &AppState, reason: Option<&str>) -> AppResult<NodeScreenSnapshot> {
    let started = Instant::now();
    let reason = reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or("unspecified");
    let client = state.client_core_snapshot();
    let Some(profile_id) = client.scope.profile_id.clone() else {
        let snapshot = NodeScreenSnapshot {
            revision: client.revision,
            scope: client.scope,
            source_status: client.source_status,
            groups: Vec::new(),
            nodes: Vec::new(),
            active_probe_jobs: client.active_probe_jobs,
        };
        log_snapshot_refresh(
            state,
            reason,
            &snapshot,
            started.elapsed().as_millis() as u64,
            false,
            None,
            None,
        );
        return Ok(snapshot);
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
    let (runtime_groups, runtime_available, runtime_error_code, runtime_error_message) =
        match adapter.policy_groups(options).await {
            Ok(groups) => (groups, true, None, None),
            Err(error) => (
                Vec::new(),
                false,
                Some(error.code.to_string()),
                Some(error.message),
            ),
        };

    // Configuration is the ordering and membership skeleton. Runtime state
    // overlays it by tag, but must never reorder groups/nodes or replace a
    // direct member list with an incomplete kernel snapshot.
    let merged_groups = merge_policy_groups(config_groups, runtime_groups);
    let groups: Vec<_> = merged_groups
        .iter()
        .map(|group| group_snapshot(&client.scope, group, runtime_available))
        .collect();
    let mut nodes = project_nodes(
        &client.scope,
        config_nodes,
        &merged_groups,
        runtime_available,
    );
    let node_indexes: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.tag.clone(), index))
        .collect();

    for job in state.list_client_probe_jobs(Some(profile_id.0.clone())) {
        if job.scope != client.scope {
            continue;
        }
        if job.state == ProbeJobState::Running {
            for target in &job.target_tags {
                if let Some(node) = node_indexes
                    .get(target)
                    .and_then(|index| nodes.get_mut(*index))
                {
                    node.active_probe_job_ids.push(job.id);
                }
            }
        }
    }
    for observation in state.client_probe_observations_for_config(&client.scope) {
        let Some(node) = node_indexes
            .get(&observation.target_tag)
            .and_then(|index| nodes.get_mut(*index))
        else {
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

    // One failed runtime policy query means this projection is stale, not that
    // the Client Core or Zero process became unavailable. Runtime freshness is
    // already represented on each node/group through `runtime_available`.
    let source_status = snapshot_source_status(client.source_status, runtime_available);
    for node in &mut nodes {
        node.active_probe_job_ids.sort_unstable();
        node.active_probe_job_ids.dedup();
        node.action_valid = source_status == SourceStatus::Ready && action_valid(&node.protocol);
    }

    let snapshot = NodeScreenSnapshot {
        revision: client.revision,
        scope: client.scope,
        source_status,
        groups,
        nodes,
        active_probe_jobs: client.active_probe_jobs,
    };
    log_snapshot_refresh(
        state,
        reason,
        &snapshot,
        started.elapsed().as_millis() as u64,
        runtime_available,
        runtime_error_code.as_deref(),
        runtime_error_message.as_deref(),
    );
    Ok(snapshot)
}

fn snapshot_source_status(client_status: SourceStatus, _runtime_available: bool) -> SourceStatus {
    client_status
}

fn log_snapshot_refresh(
    state: &AppState,
    reason: &str,
    snapshot: &NodeScreenSnapshot,
    duration_ms: u64,
    runtime_groups_available: bool,
    runtime_error_code: Option<&str>,
    runtime_error_message: Option<&str>,
) {
    logs::znet_log_fields(
        Some(state),
        LogLevel::Debug,
        if runtime_groups_available {
            "节点页面快照刷新完成"
        } else {
            "节点页面快照刷新使用静态配置回退"
        },
        serde_json::json!({
            "schema": "znet.node-screen.v1",
            "area": "nodes",
            "operation": "node_screen.refresh",
            "reason": reason,
            "durationMs": duration_ms,
            "revision": snapshot.revision.0,
            "profileId": snapshot.scope.profile_id.as_ref().map(|profile| profile.0.as_str()),
            "configRevision": snapshot.scope.config_revision.0,
            "coreInstanceId": snapshot.scope.core_instance_id.0,
            "sourceStatus": snapshot.source_status,
            "runtimeGroupsAvailable": runtime_groups_available,
            "runtimeErrorCode": runtime_error_code,
            "runtimeErrorMessage": runtime_error_message,
            "groupCount": snapshot.groups.len(),
            "nodeCount": snapshot.nodes.len(),
            "activeProbeJobCount": snapshot.active_probe_jobs.len(),
            "outcome": if runtime_groups_available { "fresh" } else { "stale_fallback" },
        }),
    );
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

fn merge_policy_groups(
    config_groups: Vec<GuiPolicyGroup>,
    runtime_groups: Vec<GuiPolicyGroup>,
) -> Vec<GuiPolicyGroup> {
    if config_groups.is_empty() {
        return runtime_groups;
    }

    let mut groups = config_groups;
    let mut indexes: HashMap<_, _> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| (group.name.clone(), index))
        .collect();
    for runtime in runtime_groups {
        let Some(index) = indexes.get(&runtime.name).copied() else {
            indexes.insert(runtime.name.clone(), groups.len());
            groups.push(runtime);
            continue;
        };
        let config = &mut groups[index];
        let runtime_members: HashMap<_, _> = runtime
            .outbounds
            .into_iter()
            .map(|member| (member.tag.clone(), member))
            .collect();

        // Preserve config ordering and direct-membership semantics. Runtime
        // snapshots only enrich members that already exist in the skeleton.
        for member in &mut config.outbounds {
            let Some(runtime_member) = runtime_members.get(&member.tag) else {
                continue;
            };
            member.kind = runtime_member.kind.clone().or_else(|| member.kind.clone());
            member.selected = runtime_member.selected;
            member.alive = runtime_member.alive;
            member.delay_ms = runtime_member.delay_ms;
            member.last_checked_unix_ms = runtime_member.last_checked_unix_ms;
            member.last_error = runtime_member.last_error.clone();
        }
        if !runtime.kind.trim().is_empty() && runtime.kind != "unknown" {
            config.kind = runtime.kind;
        }
        config.selected = runtime.selected.or_else(|| config.selected.clone());
        config.available = runtime.available;
        config.reason = runtime.reason;
    }
    groups
}

fn group_snapshot(
    scope: &crate::client_core::ClientScope,
    group: &GuiPolicyGroup,
    runtime_available: bool,
) -> NodeGroupSnapshot {
    NodeGroupSnapshot {
        id: scope
            .policy_id(group.name.clone())
            .expect("active profile checked before group projection"),
        tag: group.name.clone(),
        kind: group.kind.clone(),
        selected: group.selected.clone(),
        member_tags: group
            .outbounds
            .iter()
            .map(|member| member.tag.clone())
            .collect(),
        runtime_available,
        available: group.available,
        reason: group.reason.clone(),
    }
}

fn project_nodes(
    scope: &crate::client_core::ClientScope,
    config_nodes: Vec<ConfigProxyNode>,
    groups: &[GuiPolicyGroup],
    runtime_available: bool,
) -> Vec<NodeSnapshot> {
    let mut nodes = Vec::new();
    let mut indexes = HashMap::<String, usize>::new();

    // Keep the exact outbound order from the active configuration.
    for config_node in config_nodes {
        if indexes.contains_key(&config_node.tag) {
            continue;
        }
        indexes.insert(config_node.tag.clone(), nodes.len());
        nodes.push(node_from_config(scope, config_node, runtime_available));
    }

    // Append missing direct members in group/member declaration order. This
    // includes a group tag used as a member of another group.
    for group in groups {
        for member in &group.outbounds {
            let index = if let Some(index) = indexes.get(&member.tag).copied() {
                index
            } else {
                let index = nodes.len();
                indexes.insert(member.tag.clone(), index);
                nodes.push(node_from_member(scope, member, runtime_available));
                index
            };
            apply_member_state(&mut nodes[index], group, member, runtime_available);
        }
    }

    // A nested group is a first-class direct member card. Its own policy kind
    // and selected member determine the card state, not the parent's often
    // incomplete member metadata.
    for group in groups {
        let Some(index) = indexes.get(&group.name).copied() else {
            continue;
        };
        let node = &mut nodes[index];
        node.protocol = group.kind.clone();
        node.alive = None;
        if let Some(selected) = group
            .selected
            .as_deref()
            .and_then(|selected| group.outbounds.iter().find(|member| member.tag == selected))
        {
            node.latency_ms = selected.delay_ms.or(node.latency_ms);
            node.last_observed_at_unix_ms = selected
                .last_checked_unix_ms
                .or(node.last_observed_at_unix_ms);
            if selected.last_checked_unix_ms.is_some() {
                node.last_observation_source = Some(NodeObservationSource::RuntimeSnapshot);
            }
        }
    }

    nodes
}

fn node_from_member(
    scope: &crate::client_core::ClientScope,
    member: &GuiPolicyMember,
    runtime_available: bool,
) -> NodeSnapshot {
    NodeSnapshot {
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
        runtime_available,
        alive: member.alive,
        latency_ms: member.delay_ms,
        last_observed_at_unix_ms: member.last_checked_unix_ms,
        last_observation_source: member
            .last_checked_unix_ms
            .map(|_| NodeObservationSource::RuntimeSnapshot),
        active_probe_job_ids: Vec::new(),
        history: Vec::new(),
        action_valid: false,
    }
}

fn apply_member_state(
    node: &mut NodeSnapshot,
    group: &GuiPolicyGroup,
    member: &GuiPolicyMember,
    runtime_available: bool,
) {
    if !node.group_tags.contains(&group.name) {
        node.group_tags.push(group.name.clone());
    }
    if group.selected.as_deref() == Some(member.tag.as_str())
        && !node.selected_in.contains(&group.name)
    {
        node.selected_in.push(group.name.clone());
    }
    node.runtime_available = runtime_available;
    if node.protocol == "unknown" {
        if let Some(kind) = member.kind.as_ref().filter(|kind| !kind.trim().is_empty()) {
            node.protocol = kind.clone();
        }
    }
    let use_observation = match (node.last_observed_at_unix_ms, member.last_checked_unix_ms) {
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

fn action_valid(protocol: &str) -> bool {
    !matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "direct" | "block" | "reject" | "dns" | "selector" | "fallback" | "load_balance"
    )
}

#[cfg(test)]
mod tests {
    use super::{action_valid, merge_policy_groups, project_nodes, snapshot_source_status};
    use crate::client_core::{
        ClientScope, ConfigRevision, CoreInstanceId, ProfileId, SourceStatus,
    };
    use crate::models::gui_core::{ConfigProxyNode, GuiPolicyGroup, GuiPolicyMember};

    fn scope() -> ClientScope {
        ClientScope {
            profile_id: Some(ProfileId("profile-a".to_string())),
            config_revision: ConfigRevision(10),
            core_instance_id: CoreInstanceId(2),
        }
    }

    fn member(
        tag: &str,
        kind: Option<&str>,
        selected: bool,
        delay_ms: Option<u64>,
    ) -> GuiPolicyMember {
        GuiPolicyMember {
            tag: tag.to_string(),
            kind: kind.map(str::to_string),
            selected,
            alive: delay_ms.map(|_| true),
            delay_ms,
            last_checked_unix_ms: delay_ms.map(|_| 1_000),
            last_error: None,
        }
    }

    fn group(name: &str, kind: &str, members: Vec<GuiPolicyMember>) -> GuiPolicyGroup {
        GuiPolicyGroup {
            name: name.to_string(),
            kind: kind.to_string(),
            selected: None,
            outbounds: members,
            available: true,
            reason: None,
        }
    }

    fn config_node(tag: &str, protocol: &str) -> ConfigProxyNode {
        ConfigProxyNode {
            tag: tag.to_string(),
            protocol: protocol.to_string(),
            is_selector: false,
            server: Some(format!("{tag}.example.test")),
            port: Some(443),
            udp: Some(true),
            network: Some("tcp".to_string()),
            tls: Some(true),
            sni: Some(format!("{tag}.example.test")),
            cipher: Some("aes-256-gcm".to_string()),
        }
    }

    #[test]
    fn runtime_query_failure_does_not_mark_client_core_unavailable() {
        assert_eq!(
            snapshot_source_status(SourceStatus::Ready, false),
            SourceStatus::Ready
        );
        assert_eq!(
            snapshot_source_status(SourceStatus::Degraded, false),
            SourceStatus::Degraded
        );
        assert_eq!(
            snapshot_source_status(SourceStatus::Offline, true),
            SourceStatus::Offline
        );
    }

    #[test]
    fn config_and_runtime_groups_merge_into_one_authoritative_projection() {
        let scope = scope();
        let config = vec![group(
            "proxy",
            "selector",
            vec![
                member("second", Some("trojan"), false, None),
                member("first", Some("shadowsocks"), false, None),
            ],
        )];
        let mut runtime_group = group(
            "proxy",
            "selector",
            vec![member("first", Some("proxy"), true, Some(37))],
        );
        runtime_group.selected = Some("first".to_string());
        let groups = merge_policy_groups(config, vec![runtime_group]);
        let nodes = project_nodes(
            &scope,
            vec![
                config_node("second", "trojan"),
                config_node("first", "shadowsocks"),
            ],
            &groups,
            true,
        );

        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.selected.as_deref(), Some("first"));
        assert_eq!(
            group
                .outbounds
                .iter()
                .map(|member| member.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["second", "first"]
        );
        let node = nodes.iter().find(|node| node.tag == "first").unwrap();
        assert_eq!(node.id.profile_id.0, "profile-a");
        assert_eq!(node.group_tags, vec!["proxy"]);
        assert_eq!(node.selected_in, vec!["proxy"]);
        assert_eq!(node.latency_ms, Some(37));
        assert_eq!(node.server.as_deref(), Some("first.example.test"));
        assert!(node.runtime_available);
        assert_eq!(
            nodes
                .iter()
                .map(|node| node.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["second", "first"]
        );
    }

    #[test]
    fn group_and_member_order_survive_runtime_overlay() {
        let config = vec![
            group(
                "Node Select",
                "selector",
                vec![
                    member("Auto Select", None, false, None),
                    member("HK", Some("shadowsocks"), false, None),
                ],
            ),
            group(
                "Auto Select",
                "url_test",
                vec![
                    member("HK", Some("shadowsocks"), false, None),
                    member("US", Some("trojan"), false, None),
                ],
            ),
        ];
        let runtime = vec![
            group(
                "Auto Select",
                "url_test",
                vec![member("US", Some("proxy"), false, Some(80))],
            ),
            group(
                "Node Select",
                "selector",
                vec![member("HK", Some("proxy"), false, Some(30))],
            ),
        ];

        let groups = merge_policy_groups(config, runtime);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Node Select", "Auto Select"]
        );
        assert_eq!(
            groups[0]
                .outbounds
                .iter()
                .map(|member| member.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["Auto Select", "HK"]
        );
        assert_eq!(
            groups[1]
                .outbounds
                .iter()
                .map(|member| member.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["HK", "US"]
        );
    }

    #[test]
    fn nested_group_is_projected_as_a_direct_member_node() {
        let groups = vec![
            group(
                "Node Select",
                "selector",
                vec![
                    member("Auto Select", None, false, None),
                    member("HK", Some("shadowsocks"), false, None),
                ],
            ),
            group(
                "Auto Select",
                "url_test",
                vec![
                    member("HK", Some("shadowsocks"), false, None),
                    member("US", Some("trojan"), false, None),
                ],
            ),
        ];
        let nodes = project_nodes(
            &scope(),
            vec![
                config_node("HK", "shadowsocks"),
                config_node("US", "trojan"),
            ],
            &groups,
            false,
        );

        assert_eq!(
            nodes
                .iter()
                .map(|node| node.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["HK", "US", "Auto Select"]
        );
        let nested = nodes.iter().find(|node| node.tag == "Auto Select").unwrap();
        assert_eq!(nested.protocol, "url_test");
        assert_eq!(nested.group_tags, vec!["Node Select"]);
    }

    #[test]
    fn zero_config_shape_keeps_declared_order_and_nested_group_card() {
        let content = serde_json::json!({
            "outbounds": [
                { "tag": "direct", "protocol": { "type": "direct" } },
                { "tag": "HK", "protocol": { "type": "shadowsocks", "server": "hk.test", "port": 443 } },
                { "tag": "US", "protocol": { "type": "trojan", "server": "us.test", "port": 443 } }
            ],
            "outbound_groups": [
                {
                    "tag": "Node Select",
                    "type": "selector",
                    "outbounds": ["Auto Select", "HK", "US", "direct"]
                },
                {
                    "tag": "Auto Select",
                    "type": "url_test",
                    "outbounds": ["HK", "US"]
                }
            ]
        });
        let config_nodes = crate::kernel::zero::config::proxy_nodes_from_config(&content);
        let config_groups = crate::kernel::zero::config::policy_groups_from_config(&content);
        let groups = merge_policy_groups(config_groups, Vec::new());
        let nodes = project_nodes(&scope(), config_nodes, &groups, false);

        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Node Select", "Auto Select"]
        );
        assert_eq!(
            groups[0]
                .outbounds
                .iter()
                .map(|member| member.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["Auto Select", "HK", "US", "direct"]
        );
        assert_eq!(
            nodes
                .iter()
                .map(|node| node.tag.as_str())
                .collect::<Vec<_>>(),
            vec!["direct", "HK", "US", "Auto Select"]
        );
        let hk = nodes.iter().find(|node| node.tag == "HK").unwrap();
        assert_eq!(hk.protocol, "shadowsocks");
        assert_eq!(hk.server.as_deref(), Some("hk.test"));
        let nested = nodes.iter().find(|node| node.tag == "Auto Select").unwrap();
        assert_eq!(nested.protocol, "url_test");
    }

    #[test]
    fn action_validity_rejects_policy_and_special_outbounds() {
        assert!(action_valid("shadowsocks"));
        assert!(!action_valid("selector"));
        assert!(!action_valid("DIRECT"));
        assert!(!action_valid("block"));
    }
}
