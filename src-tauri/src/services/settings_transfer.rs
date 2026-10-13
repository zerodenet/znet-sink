//! Detached preview and local publication. No runtime command, restart or rollback.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::models::settings_transfer::{
    PortablePreferences, SettingsDocument, TransferExport, TransferPreview, TransferReceipt, SCHEMA,
};
use crate::services::{
    app_config,
    common::{lock, now_unix_ms},
};
use crate::state::app_state::AppState;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
mod validation;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_PLANS: usize = 16;
const TTL_MS: u64 = 5 * 60 * 1000;
static NEXT_PLAN: AtomicU64 = AtomicU64::new(1);
static PLANS: OnceLock<Mutex<BTreeMap<String, Plan>>> = OnceLock::new();
#[derive(Clone)]
struct Plan {
    owner: String,
    current: AppConfig,
    candidate: AppConfig,
    preview: TransferPreview,
    expires: u64,
    receipt: Option<TransferReceipt>,
    source_version: Option<(String, u64)>,
}
fn source_version(state: &AppState) -> AppResult<Option<(String, u64)>> {
    Ok(lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .map(|p| (p.id.clone(), p.updated_at_unix_ms)))
}
fn plans() -> &'static Mutex<BTreeMap<String, Plan>> {
    PLANS.get_or_init(Default::default)
}
fn fingerprint(config: &AppConfig) -> AppResult<String> {
    let bytes = serde_json::to_vec(config).map_err(|_| AppError::internal("设置版本计算失败"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn export(current: &AppConfig) -> AppResult<TransferExport> {
    let mut omitted = Vec::new();
    // The v1 migration whitelist does not transfer release channels/scheduler intent.
    if current.release_preferences != Default::default() {
        omitted.push("releasePreferences".into());
    }
    if current.mobile_notifications_enabled {
        omitted.push("mobileNotificationsEnabled".into());
    }
    let mut edits = BTreeMap::new();
    // Export only explicitly owned global choices. Never project a subscription body.
    for (field, value) in current.client_edits.iter().flat_map(|v| v.iter()) {
        if validation::validate_field(field, value, current).is_ok() {
            edits.insert(field.clone(), value.clone());
        } else {
            // No raw unknown keys or rejected values enter diagnostics/export metadata.
            omitted.push(if validation::FIELDS.contains(&field.as_str()) {
                field.clone()
            } else {
                "不可迁移的客户端字段".into()
            });
        }
    }
    let theme = current.ui.theme.trim().to_ascii_lowercase();
    let log_level = current.logs.level.trim().to_ascii_lowercase();
    let preferences = PortablePreferences {
        theme: if ["system", "light", "dark"].contains(&theme.as_str()) {
            Some(theme)
        } else {
            omitted.push("preferences.theme".into());
            None
        },
        log_level: if ["trace", "debug", "info", "warn", "error"].contains(&log_level.as_str()) {
            Some(log_level)
        } else {
            omitted.push("preferences.logLevel".into());
            None
        },
        log_max_entries: if (1..=100_000).contains(&current.logs.max_entries) {
            Some(current.logs.max_entries)
        } else {
            omitted.push("preferences.logMaxEntries".into());
            None
        },
    };
    omitted.sort();
    omitted.dedup();
    let document = SettingsDocument {
        schema_version: SCHEMA.into(),
        preferences,
        client_edits: edits,
    };
    let content = serde_json::to_string_pretty(&document)
        .map_err(|_| AppError::internal("设置导出序列化失败"))?;
    if content.len() > MAX_BYTES {
        return Err(validation::invalid("document.size"));
    }
    Ok(TransferExport {
        expected_revision: fingerprint(current)?,
        document: content,
        omitted_fields: omitted,
    })
}

pub(crate) fn parse(content: &str) -> AppResult<SettingsDocument> {
    if content.len() > MAX_BYTES {
        return Err(validation::invalid("document.size"));
    }
    let value: Value =
        serde_json::from_str(content).map_err(|_| validation::invalid("document"))?;
    if let Some(preferences) = value.get("preferences").and_then(Value::as_object) {
        for (field, value) in preferences {
            if value.is_null() {
                return Err(validation::invalid(match field.as_str() {
                    "theme" => "preferences.theme",
                    "logLevel" => "preferences.logLevel",
                    "logMaxEntries" => "preferences.logMaxEntries",
                    _ => "preferences",
                }));
            }
        }
    }
    let document: SettingsDocument =
        serde_json::from_value(value).map_err(|_| validation::invalid("document"))?;
    if document.schema_version != SCHEMA {
        return Err(validation::invalid("schemaVersion"));
    }
    Ok(document)
}

/// Portable changes merge into the existing sparse store; excluded fields survive.
pub(crate) fn candidate(
    current: &AppConfig,
    document: &SettingsDocument,
    profile: Option<&str>,
    source: &Value,
) -> AppResult<AppConfig> {
    for (key, value) in &document.client_edits {
        validation::validate_field(key, value, current)?;
    }
    if document
        .preferences
        .log_max_entries
        .is_some_and(|v| v == 0 || v > 100_000)
    {
        return Err(validation::invalid("preferences.logMaxEntries"));
    }
    let preferences: AppConfigPatch = serde_json::from_value(serde_json::json!({
        "ui":document.preferences.theme.as_ref().map(|theme|serde_json::json!({"theme":theme})).unwrap_or_else(||serde_json::json!({})),
        "logs":{
            "level":document.preferences.log_level,
            "maxEntries":document.preferences.log_max_entries,
        }
    })).map_err(|_|validation::invalid("preferences"))?;
    let mut next = app_config::prepare_update(current, preferences)
        .map_err(|_| validation::invalid("preferences"))?;
    if document.client_edits.is_empty() {
        // Preferences do not migrate network ownership or create runtime work.
        return Ok(next);
    }
    let mut edits = BTreeMap::new();
    for (key, value) in &document.client_edits {
        let (section, field) = key.split_once('.').unwrap_or((key.as_str(), ""));
        let mut patch = serde_json::json!({});
        patch[section] = if field.is_empty() {
            value.clone()
        } else {
            serde_json::json!({field:value})
        };
        let patch: AppConfigPatch =
            serde_json::from_value(patch).map_err(|_| validation::invalid(key))?;
        let normalized =
            app_config::prepare_update(current, patch).map_err(|_| validation::invalid(key))?;
        let values = serde_json::to_value(normalized).map_err(|_| validation::invalid(key))?;
        edits.insert(
            key.clone(),
            if field.is_empty() {
                values[section].clone()
            } else {
                values[section][field].clone()
            },
        );
    }
    // Existing ownership chain migrates legacy explicit choices once and keeps endpoint overrides.
    next = crate::configuration::local_edits::candidate(&next, profile.unwrap_or(""), edits, &[])
        .map_err(|_| validation::invalid("clientEdits"))?;
    let effective: AppConfig = if profile.is_some() {
        serde_json::from_value(crate::configuration::local_edits::settings(
            &next, profile, source,
        )?)
        .map_err(|_| validation::invalid("clientEdits"))?
    } else {
        let mut values =
            serde_json::to_value(&next).map_err(|_| validation::invalid("clientEdits"))?;
        for (key, value) in next
            .client_edits
            .iter()
            .flat_map(|v| v.iter())
            .filter(|(key, _)| validation::FIELDS.contains(&key.as_str()))
        {
            if let Some((section, field)) = key.split_once('.') {
                values[section][field] = value.clone();
            } else {
                values[key] = value.clone();
            }
        }
        serde_json::from_value(values).map_err(|_| validation::invalid("clientEdits"))?
    };
    validate_combined(&effective)?;
    if !document.client_edits.is_empty() && profile.is_some() {
        let (scoped, edited) = crate::configuration::local_edits::resolve(&next, profile, source)?;
        let composed = crate::configuration::composition::finalize(
            source,
            edited,
            &crate::configuration::composition::Inputs {
                source_profile_id: profile.map(str::to_owned),
                app: scoped,
                supports_tolerance: true,
                selections: BTreeMap::new(),
            },
        )?;
        // The actual Zero parser retains protocol ownership and validates the native schema.
        zero_config::RuntimeConfig::parse(&composed.config.to_string())
            .map_err(|_| validation::invalid("nativeConfiguration"))?;
    }
    Ok(next)
}

fn validate_combined(effective: &AppConfig) -> AppResult<()> {
    let (address, _) =
        crate::services::kernel_settings::validate_cidr(&effective.tun.addr, "tun.addr")
            .map_err(|_| validation::invalid("tun.addr"))?;
    let mut owned = vec![address];
    if let Some(secondary) = effective.tun.secondary_addr.as_deref() {
        let (secondary, _) =
            crate::services::kernel_settings::validate_cidr(secondary, "tun.secondaryAddr")
                .map_err(|_| validation::invalid("tun.secondaryAddr"))?;
        if !effective.tun.dual_stack || address.is_ipv4() == secondary.is_ipv4() {
            return Err(validation::invalid("tun.secondaryAddr"));
        }
        owned.push(secondary);
    } else if effective.tun.dual_stack {
        owned.push(if address.is_ipv4() {
            "fd66::1".parse().expect("client IPv6 default")
        } else {
            "10.66.0.1".parse().expect("client IPv4 default")
        });
    }
    owned.extend(
        owned
            .clone()
            .into_iter()
            .filter_map(crate::services::kernel_settings::next_ip),
    );
    if let Some(dns) = &effective.dns.config {
        dns.validate_client_shape()
            .map_err(|_| validation::invalid("dns.config"))?;
        dns.validate_tun_owned_addresses(&owned)
            .map_err(|_| validation::invalid("dns.config.answer"))?;
    } else if effective.dns.enabled {
        return Err(validation::invalid("dns.config"));
    }
    Ok(())
}

pub(crate) fn preview(state: &AppState, owner: &str, content: &str) -> AppResult<TransferPreview> {
    let document = parse(content)?;
    let current = lock(state.app_config(), "app_config")?.clone();
    let scope = state.client_core_snapshot().scope;
    let source_version = source_version(state)?;
    let (profile, source) = crate::configuration::local_edits::context(state)?;
    let candidate = candidate(&current, &document, profile.as_deref(), &source)?;
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    for key in document.client_edits.keys() {
        if current
            .client_edits
            .as_ref()
            .and_then(|edits| edits.get(key))
            != candidate
                .client_edits
                .as_ref()
                .and_then(|edits| edits.get(key))
        {
            changed.push(key.clone());
        } else {
            unchanged.push(key.clone());
        }
    }
    for (present, key, differs) in [
        (
            document.preferences.theme.is_some(),
            "preferences.theme",
            current.ui.theme != candidate.ui.theme,
        ),
        (
            document.preferences.log_level.is_some(),
            "preferences.logLevel",
            current.logs.level != candidate.logs.level,
        ),
        (
            document.preferences.log_max_entries.is_some(),
            "preferences.logMaxEntries",
            current.logs.max_entries != candidate.logs.max_entries,
        ),
    ] {
        if present {
            if differs {
                changed.push(key.into());
            } else {
                unchanged.push(key.into());
            }
        }
    }
    let runtime_required = current.client_edits != candidate.client_edits;
    let expires = now_unix_ms().saturating_add(TTL_MS);
    let id = format!(
        "settings-transfer-{}-{}",
        now_unix_ms(),
        NEXT_PLAN.fetch_add(1, Ordering::Relaxed)
    );
    let preview = TransferPreview { plan_id:id.clone(), expected_revision:fingerprint(&current)?, target_revision:fingerprint(&candidate)?, scope, source_revision:source_version.as_ref().map(|(_,revision)|revision.to_string()), changed_fields:changed, unchanged_fields:unchanged, expires_at_unix_ms:expires.to_string(), runtime_required, compatibility_pending: runtime_required && profile.is_none(), notice: "仅覆盖预览列出的客户端字段；配置正文、端点偏好、秘密、授权和接管意图保留。确认后原子保存本地设置，运行配置需另行预检和应用。".into() };
    let mut all = lock(plans(), "settings_transfer")?;
    all.retain(|_, plan| plan.expires > now_unix_ms());
    if all.len() >= MAX_PLANS {
        return Err(AppError::invalid_argument(
            "待确认的设置迁移过多，请稍后重新预览",
        ));
    }
    all.insert(
        id,
        Plan {
            owner: owner.into(),
            current,
            candidate,
            preview: preview.clone(),
            expires,
            receipt: None,
            source_version,
        },
    );
    Ok(preview)
}

fn plan(owner: &str, id: &str) -> AppResult<Plan> {
    let plan = lock(plans(), "settings_transfer")?
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::not_found("settings_transfer", "plan"))?;
    if plan.owner != owner || plan.expires <= now_unix_ms() {
        return Err(AppError::conflict(
            "settings_transfer",
            "plan",
            "预览已过期或来源窗口不同，请重新预览",
        ));
    }
    Ok(plan)
}

pub(crate) fn commit(state: &AppState, owner: &str, id: &str) -> AppResult<TransferReceipt> {
    commit_with(state, owner, id, |expected, candidate| {
        app_config::compare_and_replace_local(state, expected, candidate)
    })
}
fn commit_with(
    state: &AppState,
    owner: &str,
    id: &str,
    publish: impl FnOnce(&AppConfig, AppConfig) -> AppResult<()>,
) -> AppResult<TransferReceipt> {
    let plan = plan(owner, id)?;
    if let Some(receipt) = plan.receipt {
        return Ok(receipt);
    }
    state.ensure_client_scope(Some(&plan.preview.scope))?;
    if source_version(state)? != plan.source_version {
        return Err(AppError::conflict(
            "settings_transfer",
            "source",
            "来源版本已变化，请重新预览导入范围",
        ));
    }
    publish(&plan.current, plan.candidate.clone())?;
    let receipt = TransferReceipt {
        plan_id: id.into(),
        local_state: "saved",
        runtime_state: if plan.preview.runtime_required {
            "pending-apply"
        } else {
            "not-required"
        },
        saved_revision: plan.preview.target_revision,
        theme: plan.candidate.ui.theme,
        notice: if plan.preview.runtime_required {
            "已保存本地设置，待运行应用；没有提交运行命令或启动 VPN。"
        } else {
            "应用偏好已保存；不需要更改运行配置。"
        }
        .into(),
    };
    if let Some(plan) = lock(plans(), "settings_transfer")?.get_mut(id) {
        plan.receipt = Some(receipt.clone());
    }
    Ok(receipt)
}

pub(crate) fn status(state: &AppState, owner: &str, id: &str) -> AppResult<TransferReceipt> {
    let plan = plan(owner, id)?;
    if let Some(receipt) = plan.receipt {
        return Ok(receipt);
    }
    let current = lock(state.app_config(), "app_config")?.clone();
    let unchanged = current == plan.current;
    Ok(TransferReceipt {
        plan_id: id.into(),
        local_state: if unchanged {
            "not-committed"
        } else {
            "unknown"
        },
        runtime_state: "unknown",
        saved_revision: fingerprint(&current)?,
        theme: current.ui.theme,
        notice: if unchanged {
            "当前设置仍匹配预览基线；尚无已保存回执。"
        } else {
            "当前设置已变化，但缺少此操作的确认回执；请重新读取设置，不会重放导入。"
        }
        .into(),
    })
}

#[cfg(test)]
mod tests;
