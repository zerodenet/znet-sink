use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use zero_rule::protocol::{decode_json, encode_json};
use zero_rule::zrs::{encode, verify, RuleSetMetadata, VerifyMode};
use zero_rule::RuleSetCompiler;

use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{
    RuleSetKernelPayload, RuleSetProfile, RuleSetSource, RuleSetSourceState, RuleSetSyncAllOutcome,
    RuleSetUpsert, ZrsArtifact,
};
use crate::services::common::{
    generated_store_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::{data_dir, domain_store};
use crate::state::app_state::AppState;

const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MIN_UPDATE_INTERVAL_SECS: u64 = 60;
const AUTO_UPDATE_TICK_SECS: u64 = 60;
const DEFAULT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<RuleSetProfile>> {
    Ok(lock(state.rule_sets(), "rule_set")?.clone())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id))
}

pub async fn upsert(state: State<'_, AppState>, input: RuleSetUpsert) -> AppResult<RuleSetProfile> {
    let name = normalize_required(input.name, "name")?;
    let id = normalize_optional(input.id).unwrap_or_else(|| generated_store_id("rule-set"));
    let previous = lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .map(|item| {
            (
                item.source.clone(),
                item.source_state.clone(),
                item.last_sync_at_unix_ms,
            )
        });
    let source = input.source.map(normalize_source).transpose()?;
    let same_source = previous
        .as_ref()
        .and_then(|(source, _, _)| source.as_ref())
        .zip(source.as_ref())
        .is_some_and(|(old, new)| old.url == new.url);
    let (semantic_ir, source_state, did_sync) = match input.semantic_ir {
        Some(ir) => (
            canonical_ir(ir, &name)?,
            if same_source {
                previous
                    .as_ref()
                    .map(|(_, state, _)| state.clone())
                    .unwrap_or_default()
            } else {
                RuleSetSourceState::default()
            },
            false,
        ),
        None => {
            let source = source.as_ref().ok_or_else(|| {
                AppError::invalid_argument("semanticIr is required without a subscription source")
            })?;
            let FetchOutcome::Modified(resource) = fetch_source(source, None).await? else {
                return Err(AppError::internal(
                    "new rule source unexpectedly returned not modified",
                ));
            };
            (
                convert_source(&resource.content, &source.format, &name)?,
                resource.state,
                true,
            )
        }
    };
    let artifact = build_zrs_artifact(&id, &semantic_ir)?;
    let now = now_unix_ms();
    let profile = RuleSetProfile {
        id: id.clone(),
        name,
        enabled: input.enabled.unwrap_or(true),
        semantic_ir,
        source: source.clone(),
        source_state,
        artifact: Some(artifact),
        updated_at_unix_ms: now,
        last_sync_at_unix_ms: if did_sync {
            Some(now)
        } else if same_source {
            previous.and_then(|(_, _, last_sync)| last_sync)
        } else {
            None
        },
        last_error: None,
    };

    let mut items = lock(state.rule_sets(), "rule_set")?;
    match items.iter_mut().find(|item| item.id == id) {
        Some(existing) => *existing = profile.clone(),
        None => items.push(profile.clone()),
    }
    domain_store::save_rule_sets(&items)?;
    Ok(profile)
}

/// Download an external rule file, adapt it into the canonical kernel semantics,
/// then build and atomically publish a new immutable ZRS generation.
pub async fn update(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    update_by_id(state.inner(), id)
        .await
        .map(|(profile, _)| profile)
}

async fn update_by_id(state: &AppState, id: String) -> AppResult<(RuleSetProfile, bool)> {
    let id = normalize_required(id, "id")?;
    let profile = lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id.clone()))?;
    let source = profile
        .source
        .clone()
        .ok_or_else(|| AppError::invalid_argument("this rule asset has no subscription source"))?;
    let result = async {
        match fetch_source(&source, Some(&profile.source_state)).await? {
            FetchOutcome::NotModified(source_state) => Ok::<_, AppError>((None, source_state)),
            FetchOutcome::Modified(resource) => {
                let semantic_ir = convert_source(&resource.content, &source.format, &profile.name)?;
                let artifact = build_zrs_artifact(&profile.id, &semantic_ir)?;
                Ok((Some((semantic_ir, artifact)), resource.state))
            }
        }
    }
    .await;

    let mut items = lock(state.rule_sets(), "rule_set")?;
    let item = items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::not_found("rule_set", id))?;
    match result {
        Ok((changed, source_state)) => {
            let was_updated = changed.is_some();
            if let Some((semantic_ir, artifact)) = changed {
                item.semantic_ir = semantic_ir;
                item.artifact = Some(artifact);
                item.last_sync_at_unix_ms = Some(now_unix_ms());
                item.updated_at_unix_ms = now_unix_ms();
            }
            item.source_state = source_state;
            item.last_error = None;
            let updated = item.clone();
            domain_store::save_rule_sets(&items)?;
            Ok((updated, was_updated))
        }
        Err(error) => {
            // Keep the last known-good semantic rules and ZRS generation active.
            item.last_error = Some(error.message.clone());
            domain_store::save_rule_sets(&items)?;
            Err(error)
        }
    }
}

pub async fn update_all(state: State<'_, AppState>) -> AppResult<RuleSetSyncAllOutcome> {
    update_all_with_state(state.inner()).await
}

async fn update_all_with_state(state: &AppState) -> AppResult<RuleSetSyncAllOutcome> {
    let ids = lock(state.rule_sets(), "rule_set")?
        .iter()
        .filter(|item| item.enabled && item.source.is_some())
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let mut outcome = RuleSetSyncAllOutcome {
        total: ids.len(),
        updated: 0,
        unchanged: 0,
        failed: 0,
    };
    for id in ids {
        match update_by_id(state, id).await {
            Ok((_, true)) => outcome.updated += 1,
            Ok((_, false)) => outcome.unchanged += 1,
            Err(_) => outcome.failed += 1,
        }
    }
    Ok(outcome)
}

pub fn kernel_payloads(state: State<'_, AppState>) -> AppResult<Vec<RuleSetKernelPayload>> {
    let items = lock(state.rule_sets(), "rule_set")?;
    items
        .iter()
        .filter(|item| item.enabled)
        .map(|item| {
            let artifact = item.artifact.as_ref().ok_or_else(|| {
                AppError::invalid_argument(format!(
                    "rule asset '{}' has no verified ZRS artifact",
                    item.name
                ))
            })?;
            Ok(RuleSetKernelPayload {
                id: item.id.clone(),
                name: item.name.clone(),
                zrs_path: artifact.path.clone(),
                checksum: artifact.checksum,
            })
        })
        .collect()
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let mut items = lock(state.rule_sets(), "rule_set")?;
    let before = items.len();
    items.retain(|item| item.id != id);
    if items.len() == before {
        return Err(AppError::not_found("rule_set", id));
    }
    // Published generations are intentionally retained: a running kernel may still mmap them.
    domain_store::save_rule_sets(&items)
}

fn normalize_source(mut source: RuleSetSource) -> AppResult<RuleSetSource> {
    source.url = normalize_required(source.url, "source.url")?;
    if !source.url.starts_with("https://") && !source.url.starts_with("http://") {
        return Err(AppError::invalid_argument(
            "source.url must use http:// or https://",
        ));
    }
    source.format = match source.format.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => "auto".into(),
        "zero" | "zero-rule" | "zero-rule-ir-v1" => "zero-rule-ir-v1".into(),
        "clash" | "clash-yaml" | "clash-classical-yaml" => "clash-classical-yaml".into(),
        other => {
            return Err(AppError::invalid_argument(format!(
                "unsupported subscription adapter: {other}"
            )))
        }
    };
    source.update_interval_secs = match source.update_interval_secs {
        None | Some(0) => None,
        Some(seconds) if seconds < MIN_UPDATE_INTERVAL_SECS => {
            return Err(AppError::invalid_argument(format!(
                "rule update interval must be at least {MIN_UPDATE_INTERVAL_SECS} seconds"
            )))
        }
        value => value,
    };
    source.user_agent = normalize_optional(source.user_agent);
    Ok(source)
}

struct FetchedResource {
    content: String,
    state: RuleSetSourceState,
}

enum FetchOutcome {
    Modified(FetchedResource),
    NotModified(RuleSetSourceState),
}

async fn fetch_source(
    source: &RuleSetSource,
    previous: Option<&RuleSetSourceState>,
) -> AppResult<FetchOutcome> {
    let url = source.url.clone();
    let user_agent = source
        .user_agent
        .clone()
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
    let previous = previous.cloned().unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .user_agent(user_agent)
            .build()
            .map_err(|error| AppError::internal(format!("failed to build rule client: {error}")))?;
        let mut request = client.get(&url);
        if let Some(etag) = &previous.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &previous.last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
        }
        let mut response = request.send().map_err(|error| {
            AppError::internal(format!("failed to download rule source: {error}"))
        })?;
        let checked_at = now_unix_ms();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let mut state = previous;
            state.last_checked_at_unix_ms = Some(checked_at);
            return Ok(FetchOutcome::NotModified(state));
        }
        response = response.error_for_status().map_err(|error| {
            AppError::invalid_argument(format!("rule source rejected update: {error}"))
        })?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_INPUT_BYTES as u64)
        {
            return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        response
            .take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                AppError::invalid_argument(format!("failed to read rule source: {error}"))
            })?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
        }
        let content_bytes = bytes.len() as u64;
        let content_sha256 = format!("{:x}", Sha256::digest(&bytes));
        let content = String::from_utf8(bytes).map_err(|error| {
            AppError::invalid_argument(format!("rule source is not valid UTF-8: {error}"))
        })?;
        let state = RuleSetSourceState {
            etag,
            last_modified,
            content_sha256: Some(content_sha256.clone()),
            content_bytes: Some(content_bytes),
            last_checked_at_unix_ms: Some(checked_at),
        };
        if previous.content_sha256.as_deref() == Some(content_sha256.as_str()) {
            return Ok(FetchOutcome::NotModified(state));
        }
        Ok(FetchOutcome::Modified(FetchedResource { content, state }))
    })
    .await
    .map_err(|error| AppError::internal(format!("rule download worker failed: {error}")))?
}

pub fn spawn_auto_update_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let mut interval = tokio::time::interval(Duration::from_secs(AUTO_UPDATE_TICK_SECS));
        loop {
            interval.tick().await;
            run_auto_update_pass(&app).await;
        }
    });
}

async fn run_auto_update_pass(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = now_unix_ms();
    let ids = match lock(state.rule_sets(), "rule_set") {
        Ok(items) => items
            .iter()
            .filter(|item| is_due(item, now))
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        Err(_) => return,
    };
    for id in ids {
        let _ = update_by_id(state.inner(), id).await;
    }
}

fn is_due(item: &RuleSetProfile, now_ms: u64) -> bool {
    let Some(source) = &item.source else {
        return false;
    };
    let Some(interval) = source.update_interval_secs else {
        return false;
    };
    if !item.enabled {
        return false;
    }
    let last = item
        .source_state
        .last_checked_at_unix_ms
        .or(item.last_sync_at_unix_ms)
        .unwrap_or(0);
    now_ms.saturating_sub(last) >= interval.saturating_mul(1000)
}

fn canonical_ir(value: Value, display_name: &str) -> AppResult<Value> {
    let mut value = value;
    value["name"] = json!(display_name);
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| AppError::invalid_argument(format!("invalid semantic rules: {error}")))?;
    let rule_set = decode_json(&bytes)
        .map_err(|error| AppError::invalid_argument(format!("invalid Zero Rule IR: {error}")))?;
    let canonical = encode_json(&rule_set)
        .map_err(|error| AppError::internal(format!("failed to encode Zero Rule IR: {error}")))?;
    serde_json::from_slice(&canonical)
        .map_err(|error| AppError::internal(format!("failed to materialize Zero Rule IR: {error}")))
}

pub fn convert_source(content: &str, format: &str, display_name: &str) -> AppResult<Value> {
    if content.len() > MAX_INPUT_BYTES {
        return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
    }
    match format {
        "zero-rule-ir-v1" => {
            let value = serde_json::from_str(content).map_err(|error| {
                AppError::invalid_argument(format!("Zero Rule IR JSON is invalid: {error}"))
            })?;
            canonical_ir(value, display_name)
        }
        "clash-classical-yaml" => convert_clash(content, display_name),
        "auto" => serde_json::from_str(content)
            .ok()
            .and_then(|value| canonical_ir(value, display_name).ok())
            .map(Ok)
            .unwrap_or_else(|| convert_clash(content, display_name)),
        other => Err(AppError::invalid_argument(format!(
            "unsupported subscription adapter: {other}"
        ))),
    }
}

fn convert_clash(content: &str, display_name: &str) -> AppResult<Value> {
    let yaml: Value = serde_yaml::from_str(content).map_err(|error| {
        AppError::invalid_argument(format!("Clash rule YAML is invalid: {error}"))
    })?;
    let payload = yaml
        .get("payload")
        .and_then(Value::as_array)
        .or_else(|| yaml.as_array())
        .ok_or_else(|| AppError::invalid_argument("Clash rules must contain a payload array"))?;
    let mut rules = Vec::with_capacity(payload.len());
    for entry in payload {
        let raw = entry
            .as_str()
            .ok_or_else(|| AppError::invalid_argument("Clash payload entries must be strings"))?;
        let mut parts = raw.split(',').map(str::trim);
        let source_type = parts.next().unwrap_or("").to_ascii_uppercase();
        let value = parts
            .next()
            .ok_or_else(|| AppError::invalid_argument(format!("Clash rule has no value: {raw}")))?;
        let rule_type = match source_type.as_str() {
            "DOMAIN" => "domain_exact",
            "DOMAIN-SUFFIX" => "domain_suffix",
            "DOMAIN-KEYWORD" => "domain_keyword",
            "IP-CIDR" => "ipv4_cidr",
            "IP-CIDR6" => "ipv6_cidr",
            _ => {
                return Err(AppError::invalid_argument(format!(
                    "unsupported Clash rule type '{source_type}'; conversion is strict"
                )))
            }
        };
        rules.push(json!({ "type": rule_type, "value": value }));
    }
    canonical_ir(
        json!({ "version": 1, "name": display_name, "rules": rules }),
        display_name,
    )
}

fn build_zrs_artifact(id: &str, semantic_ir: &Value) -> AppResult<ZrsArtifact> {
    let ir = serde_json::to_vec(semantic_ir).map_err(|error| {
        AppError::internal(format!("failed to serialize semantic rules: {error}"))
    })?;
    let source = decode_json(&ir)
        .map_err(|error| AppError::invalid_argument(format!("invalid Zero Rule IR: {error}")))?;
    let (compiled, _) = RuleSetCompiler
        .compile(source)
        .map_err(|error| AppError::invalid_argument(format!("rule compilation failed: {error}")))?;
    let bytes = encode(&compiled)
        .map_err(|error| AppError::internal(format!("ZRS encoding failed: {error}")))?;
    let metadata = verify(&bytes, VerifyMode::FullChecksum)
        .map_err(|error| AppError::internal(format!("ZRS verification failed: {error}")))?;
    publish_zrs_in(&data_dir()?, id, &bytes, metadata)
}

fn publish_zrs_in(
    base_dir: &Path,
    id: &str,
    bytes: &[u8],
    metadata: RuleSetMetadata,
) -> AppResult<ZrsArtifact> {
    let directory = base_dir.join("rule-artifacts").join(id);
    fs::create_dir_all(&directory)
        .map_err(|error| io_error("create ZRS directory", &directory, error))?;
    let generation = now_unix_ms();
    let file_name = format!("{generation}-{:08x}.zrs", metadata.body_checksum);
    let final_path = directory.join(file_name);
    let temporary_path = directory.join(format!(".{generation}-{}.tmp", std::process::id()));
    let publish = || -> AppResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|error| io_error("create temporary ZRS", &temporary_path, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("write temporary ZRS", &temporary_path, error))?;
        file.sync_all()
            .map_err(|error| io_error("flush temporary ZRS", &temporary_path, error))?;
        let written = fs::read(&temporary_path)
            .map_err(|error| io_error("read temporary ZRS", &temporary_path, error))?;
        verify(&written, VerifyMode::FullChecksum).map_err(|error| {
            AppError::internal(format!("published ZRS verification failed: {error}"))
        })?;
        fs::rename(&temporary_path, &final_path)
            .map_err(|error| io_error("publish immutable ZRS", &final_path, error))?;
        Ok(())
    };
    if let Err(error) = publish() {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(ZrsArtifact {
        path: final_path.to_string_lossy().into_owned(),
        major_version: metadata.major_version,
        minor_version: metadata.minor_version,
        checksum: metadata.body_checksum,
        file_size: metadata.file_size,
        entry_count: metadata.entry_count(),
        built_at_unix_ms: generation,
    })
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("failed to {action}: {error}"),
        details: Some(json!({ "path": path.display().to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clash_subscription_becomes_canonical_kernel_semantics() {
        let ir = convert_source(
            "payload:\n  - DOMAIN,Api.Example.COM.\n  - IP-CIDR,10.0.0.0/8,no-resolve\n",
            "clash-classical-yaml",
            "Imported",
        )
        .unwrap();
        assert_eq!(ir["version"], 1);
        assert_eq!(ir["rules"].as_array().unwrap().len(), 2);
        assert!(!ir.to_string().contains("Clash"));
        assert!(!ir.to_string().contains("no-resolve"));
    }

    #[test]
    fn unsupported_subscription_semantics_are_rejected() {
        let error =
            convert_source("payload:\n  - GEOIP,CN\n", "clash-classical-yaml", "Bad").unwrap_err();
        assert!(error.message.contains("unsupported Clash rule type"));
    }

    #[test]
    fn semantic_rules_compile_to_verified_zrs() {
        let ir = canonical_ir(
            json!({
                "version": 1,
                "rules": [
                    {"type":"domain_suffix","value":"example.com"},
                    {"type":"ipv4_cidr","value":"10.0.0.0/8"}
                ]
            }),
            "Visual asset",
        )
        .unwrap();
        let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();
        let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
        assert_eq!(&bytes[..4], b"ZRS!");
        assert_eq!(metadata.entry_count(), 2);
    }

    #[test]
    fn zrs_generation_is_published_as_a_verified_immutable_file() {
        let root = std::env::temp_dir().join(format!(
            "znet-rule-test-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let ir = canonical_ir(
            json!({"version":1,"rules":[{"type":"domain_exact","value":"example.com"}]}),
            "Published",
        )
        .unwrap();
        let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();
        let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
        let artifact = publish_zrs_in(&root, "asset", &bytes, metadata).unwrap();
        let published = fs::read(&artifact.path).unwrap();
        verify(&published, VerifyMode::FullChecksum).unwrap();
        assert_eq!(&published[..4], b"ZRS!");
        assert!(!Path::new(&artifact.path).with_extension("tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auto_update_only_selects_due_enabled_external_assets() {
        let mut profile = RuleSetProfile {
            id: "asset".into(),
            name: "Asset".into(),
            enabled: true,
            semantic_ir: json!({"version":1,"name":"Asset","rules":[]}),
            source: Some(RuleSetSource {
                url: "https://example.com/rules".into(),
                format: "auto".into(),
                update_interval_secs: Some(3600),
                user_agent: None,
            }),
            source_state: RuleSetSourceState {
                last_checked_at_unix_ms: Some(1_000),
                ..Default::default()
            },
            artifact: None,
            updated_at_unix_ms: 1_000,
            last_sync_at_unix_ms: Some(1_000),
            last_error: None,
        };
        assert!(!is_due(&profile, 3_600_999));
        assert!(is_due(&profile, 3_601_000));
        profile.enabled = false;
        assert!(!is_due(&profile, u64::MAX));
    }
}
