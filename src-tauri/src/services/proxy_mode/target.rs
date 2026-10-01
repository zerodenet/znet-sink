use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::errors::{AppError, AppResult};
use crate::models::gui_core::GuiPolicyGroup;

// Validate the client-selected global target against the active configuration.
// The kernel remains authoritative for route validation and confirmed apply.
pub(super) fn validate(content: &Value, target: &str) -> AppResult<()> {
    let groups = crate::kernel::zero::config::policy_groups_from_config(content);
    let groups: HashMap<_, _> = groups
        .iter()
        .map(|group| (group.name.as_str(), group))
        .collect();
    let mut known: HashSet<String> = crate::kernel::zero::config::proxy_nodes_from_config(content)
        .into_iter()
        .map(|node| node.tag)
        .collect();
    // Preserve legacy flat config support in the existing mode command.
    for key in ["outbounds", "proxies"] {
        for row in content
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(tag) = row
                .get("tag")
                .or_else(|| row.get("name"))
                .and_then(Value::as_str)
            {
                known.insert(tag.to_owned());
            }
        }
    }
    if groups.get(target).is_some_and(|group| {
        matches!(
            group.kind.to_ascii_lowercase().as_str(),
            "selector" | "select"
        )
    }) {
        return Err(AppError::invalid_argument(
            "全局出口请选择节点或非 selector 节点组；手动选择组请独立管理",
        ));
    }
    visit(
        target,
        &known,
        &groups,
        &mut HashSet::new(),
        &mut HashSet::new(),
    )
}

fn visit<'a>(
    target: &'a str,
    known: &HashSet<String>,
    groups: &HashMap<&'a str, &'a GuiPolicyGroup>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> AppResult<()> {
    if visiting.contains(target) {
        return Err(AppError::invalid_argument(
            "节点组存在循环引用，无法作为全局出口",
        ));
    }
    let Some(group) = groups.get(target) else {
        return if known.contains(target) {
            Ok(())
        } else {
            Err(AppError::invalid_argument(format!(
                "全局出口引用的出站 {target} 不存在"
            )))
        };
    };
    if visited.contains(target) {
        return Ok(());
    }
    if group.outbounds.is_empty() {
        return Err(AppError::invalid_argument("节点组没有可用成员"));
    }
    visiting.insert(target);
    for member in &group.outbounds {
        visit(&member.tag, known, groups, visiting, visited)?;
    }
    visiting.remove(target);
    visited.insert(target);
    Ok(())
}
