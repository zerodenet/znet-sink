//! A public directory/navigation identity, never execution authority or a
//! runtime-success fact. This reads only AppState metadata: no source bodies,
//! paths, URLs, SQLite, composition, kernel query, file write or platform IO.
use crate::{
    errors::{AppError, AppResult},
    models::mobile_location::{Binding, Profile, Snapshot},
    services::common::lock,
    state::app_state::AppState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Serialize)]
struct DirectoryProfile<'a> {
    id: &'a str,
    name: &'a str,
    kernel: &'a str,
    format: &'a str,
    active: bool,
    updated_at_unix_ms: u64,
    managed: bool,
    has_ordinary_material: bool,
}
#[derive(Serialize)]
struct DirectorySubscription<'a> {
    id: &'a str,
    target: Option<&'a str>,
    managed: bool,
    updated_at_unix_ms: u64,
}
fn unavailable(reason: &str) -> Snapshot {
    Snapshot {
        binding: None,
        profile: None,
        reason: Some(reason.into()),
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}
pub(crate) fn snapshot(state: &AppState) -> AppResult<Snapshot> {
    // Same domain -> core order as local publication. No callback below takes
    // another AppState lock, emits events, or advances a generation.
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut ids = BTreeSet::new();
    let mut active = None;
    let mut directory = Vec::with_capacity(profiles.len());
    for profile in profiles.iter() {
        if !valid_id(&profile.id) || !ids.insert(profile.id.as_str()) {
            return Ok(unavailable(
                "配置目录身份不一致；请刷新最新配置列表后重新定位",
            ));
        }
        if profile.active && active.replace(profile).is_some() {
            return Ok(unavailable(
                "当前配置选择不一致；请刷新最新配置列表后重新定位",
            ));
        }
        directory.push(DirectoryProfile {
            id: &profile.id,
            name: &profile.name,
            kernel: &profile.kernel,
            format: &profile.format,
            active: profile.active,
            updated_at_unix_ms: profile.updated_at_unix_ms,
            managed: profile.managed_source.is_some(),
            has_ordinary_material: profile.content.is_some(),
        });
    }
    let mut subscription_ids = BTreeSet::new();
    let mut relationships = Vec::with_capacity(subscriptions.len());
    for subscription in subscriptions.iter() {
        if !valid_id(&subscription.id)
            || !subscription_ids.insert(subscription.id.as_str())
            || subscription
                .target_proxy_config_id
                .as_deref()
                .is_some_and(|id| !ids.contains(id))
        {
            return Ok(unavailable(
                "配置来源关系不一致；请刷新最新配置列表后重新定位",
            ));
        }
        relationships.push(DirectorySubscription {
            id: &subscription.id,
            target: subscription.target_proxy_config_id.as_deref(),
            managed: subscription.managed_source.is_some(),
            updated_at_unix_ms: subscription.updated_at_unix_ms,
        });
    }
    let current = state.client_core_snapshot();
    if current.scope_generation.0 == 0
        || current.scope.profile_id.as_ref().map(|id| id.0.as_str())
            != active.map(|profile| profile.id.as_str())
    {
        return Ok(unavailable(
            "当前配置与客户端作用域尚未一致；请刷新后重新定位",
        ));
    }
    let Some(active) = active else {
        return Ok(unavailable(
            "尚未选择当前配置；可进入最新配置列表添加或选择配置",
        ));
    };
    state.with_client_scope_generation(&current.scope, current.scope_generation.0, || {
        // Do not hash ClientCoreSnapshot.revision: ordinary observations are
        // independent of the directory lifecycle and must not retire a visit.
        let bytes = serde_json::to_vec(&(
            "mobile-location-v1",
            state.host_session_id()?,
            &current.scope,
            current.scope_generation.0,
            &directory,
            &relationships,
        ))
        .map_err(|_| AppError::internal("定位身份编码失败，请刷新后重试"))?;
        Ok(Snapshot {
            binding: Some(Binding {
                binding: format!("{:x}", Sha256::digest(bytes)),
                profile_id: active.id.clone(),
                config_revision: current.scope.config_revision.0.to_string(),
                core_instance_id: current.scope.core_instance_id.0.to_string(),
                scope_generation: current.scope_generation.0.to_string(),
            }),
            profile: Some(Profile {
                id: active.id.clone(),
                name: active.name.clone(),
            }),
            reason: None,
        })
    })
}

#[cfg(test)]
mod tests;
