use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{RuleSetProfile, RuleSetSource, RuleSetUpsert};
use crate::services::common::{
    generated_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::domain_store;
use crate::state::app_state::AppState;

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<RuleSetProfile>> {
    Ok(lock(state.rule_sets(), "rule_set")?.clone())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id))
}

pub fn upsert(state: State<'_, AppState>, input: RuleSetUpsert) -> AppResult<RuleSetProfile> {
    let name = normalize_required(input.name, "name")?;
    let source = normalize_source(input.source)?;
    let id = normalize_optional(input.id)
        .unwrap_or_else(|| generated_id("rule-set", state.next_record_id()));

    let profile = RuleSetProfile {
        id: id.clone(),
        name,
        format: normalize_optional(input.format).unwrap_or_else(|| "auto".to_string()),
        enabled: input.enabled.unwrap_or(true),
        source,
        updated_at_unix_ms: now_unix_ms(),
    };

    let mut rule_sets = lock(state.rule_sets(), "rule_set")?;
    match rule_sets.iter_mut().find(|item| item.id == id) {
        Some(existing) => *existing = profile.clone(),
        None => rule_sets.push(profile.clone()),
    }
    domain_store::save_rule_sets(&rule_sets)?;

    Ok(profile)
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let mut rule_sets = lock(state.rule_sets(), "rule_set")?;
    let before = rule_sets.len();
    rule_sets.retain(|profile| profile.id != id);

    if rule_sets.len() == before {
        return Err(AppError::not_found("rule_set", id));
    }
    domain_store::save_rule_sets(&rule_sets)?;

    Ok(())
}

fn normalize_source(mut source: RuleSetSource) -> AppResult<RuleSetSource> {
    source.kind = normalize_required(source.kind, "source.kind")?.to_ascii_lowercase();
    source.url = normalize_optional(source.url);
    source.path = normalize_optional(source.path);

    match source.kind.as_str() {
        "remote" => {
            let url = source.url.as_deref().ok_or_else(|| {
                AppError::invalid_argument("source.url is required for remote rule sets")
            })?;
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return Err(AppError::invalid_argument(
                    "source.url must start with http:// or https://",
                ));
            }
        }
        "file" => {
            if source.path.is_none() {
                return Err(AppError::invalid_argument(
                    "source.path is required for file rule sets",
                ));
            }
        }
        "inline" => {
            if source.content.is_none() {
                return Err(AppError::invalid_argument(
                    "source.content is required for inline rule sets",
                ));
            }
        }
        _ => {
            return Err(AppError::invalid_argument(
                "source.kind must be remote, file, or inline",
            ));
        }
    }

    Ok(source)
}
