use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zero_rule::zrs::{verify, VerifyMode};

use super::{data_dir, domain_store};
use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{
    CommonRuleAction, CommonRuleBinding, RuleSetProfile, RuleSetProvenance, RuleSetSourceState,
    RuleSetSyncAllOutcome, ZrsArtifact,
};
use crate::services::common::{lock, now_unix_ms};
use crate::state::app_state::AppState;

const MANIFEST_JSON: &str = include_str!("../../resources/builtin-rules/manifest.json");
const REMOTE_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/zerodenet/znet-sink/builtin-rules/manifest.json";
const REMOTE_ASSET_ROOT: &str =
    "https://raw.githubusercontent.com/zerodenet/znet-sink/builtin-rules";
const MAX_REMOTE_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_REMOTE_ASSET_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: String,
    version: u32,
    generated_at_unix_ms: u64,
    source_repository: String,
    source_commit: String,
    source_license: String,
    assets: Vec<ManifestAsset>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAsset {
    id: String,
    name: String,
    zrs_file: String,
    source_url: String,
    source_sha256: String,
    ir_sha256: String,
    zrs_checksum: u32,
    zrs_sha256: String,
    zrs_file_size: u64,
    entry_count: u64,
    default_action: CommonRuleAction,
    default_order: u32,
}

pub(crate) fn install_defaults(items: &mut Vec<RuleSetProfile>) -> AppResult<bool> {
    install_defaults_in(&data_dir()?, items)
}

fn install_defaults_in(base: &Path, items: &mut Vec<RuleSetProfile>) -> AppResult<bool> {
    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON)
        .map_err(|error| AppError::internal(format!("invalid built-in rule manifest: {error}")))?;
    if manifest.schema != "znet.builtin-rules/v1" || manifest.version != 2 {
        return Err(AppError::internal("unsupported built-in rule manifest"));
    }

    let mut changed = false;
    for asset in &manifest.assets {
        let existing_index = items.iter().position(|item| item.id == asset.id);
        if existing_index.is_some_and(|index| !items[index].built_in) {
            return Err(AppError::invalid_argument(format!(
                "reserved built-in rule id is already used: {}",
                asset.id
            )));
        }
        if existing_index.is_some_and(|index| profile_is_current(&items[index], &manifest, &asset))
        {
            continue;
        }

        let zrs_bytes = embedded_file(&asset.zrs_file)?;
        validate_asset(zrs_bytes, &asset)?;
        let semantic_ir = serde_json::json!({
            "version": 1,
            "name": asset.name,
            "rules": [],
        });
        let metadata = verify(zrs_bytes, VerifyMode::FullChecksum).map_err(|error| {
            AppError::internal(format!("invalid built-in ZRS '{}': {error}", asset.id))
        })?;
        let artifact_path = install_artifact(base, &asset, zrs_bytes)?;
        let provenance = RuleSetProvenance {
            repository: manifest.source_repository.clone(),
            revision: manifest.source_commit.clone(),
            license: manifest.source_license.clone(),
            source_url: asset.source_url.clone(),
            source_sha256: asset.source_sha256.clone(),
            ir_sha256: asset.ir_sha256.clone(),
        };
        let previous = existing_index.map(|index| items[index].clone());
        let profile = RuleSetProfile {
            id: asset.id.clone(),
            name: asset.name.clone(),
            enabled: previous.as_ref().map_or(true, |profile| profile.enabled),
            built_in: true,
            provenance: Some(provenance),
            managed_by_subscription_id: None,
            common_binding: previous
                .as_ref()
                .and_then(|profile| profile.common_binding.clone())
                .or_else(|| {
                    Some(CommonRuleBinding {
                        enabled: true,
                        action: asset.default_action.clone(),
                        order: asset.default_order,
                    })
                }),
            semantic_ir,
            source: None,
            source_state: RuleSetSourceState::default(),
            artifact: Some(ZrsArtifact {
                path: artifact_path.to_string_lossy().into_owned(),
                major_version: metadata.major_version,
                minor_version: metadata.minor_version,
                checksum: metadata.body_checksum,
                file_size: metadata.file_size,
                entry_count: metadata.entry_count(),
                built_at_unix_ms: manifest.generated_at_unix_ms,
            }),
            updated_at_unix_ms: manifest.generated_at_unix_ms,
            last_sync_at_unix_ms: None,
            last_error: None,
        };
        match existing_index {
            Some(index) => items[index] = profile,
            None => items.push(profile),
        }
        changed = true;
    }

    if changed {
        domain_store::save_rule_sets_to_dir(base, items)?;
    }
    Ok(changed)
}

fn profile_is_current(
    profile: &RuleSetProfile,
    manifest: &Manifest,
    asset: &ManifestAsset,
) -> bool {
    profile.built_in
        && profile.provenance.as_ref().is_some_and(|provenance| {
            provenance.revision == manifest.source_commit && provenance.ir_sha256 == asset.ir_sha256
        })
        && profile.artifact.as_ref().is_some_and(|artifact| {
            artifact.checksum == asset.zrs_checksum
                && artifact.file_size == asset.zrs_file_size
                && artifact.entry_count == asset.entry_count
                && installed_artifact_is_valid(Path::new(&artifact.path), asset)
        })
}

fn installed_artifact_is_valid(path: &Path, asset: &ManifestAsset) -> bool {
    fs::read(path).ok().is_some_and(|bytes| {
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        verify(&bytes, VerifyMode::FullChecksum)
            .ok()
            .is_some_and(|metadata| {
                sha256 == asset.zrs_sha256
                    && metadata.body_checksum == asset.zrs_checksum
                    && metadata.file_size == asset.zrs_file_size
                    && metadata.entry_count() == asset.entry_count
            })
    })
}

/// Download one client-owned, precompiled rule bundle and atomically switch
/// every built-in profile only after the complete manifest and all ZRS assets
/// have passed SHA-256 plus structural verification.
pub(crate) async fn update_all(state: &AppState) -> AppResult<RuleSetSyncAllOutcome> {
    let current_generation = lock(state.rule_sets(), "rule_set")?
        .iter()
        .filter(|item| item.built_in)
        .map(|item| item.updated_at_unix_ms)
        .max()
        .unwrap_or_default();
    let (manifest, assets) = tauri::async_runtime::spawn_blocking(fetch_remote_bundle)
        .await
        .map_err(|error| {
            AppError::internal(format!("built-in rule update worker failed: {error}"))
        })??;
    let total = manifest.assets.len();
    if manifest.generated_at_unix_ms < current_generation {
        return Ok(RuleSetSyncAllOutcome {
            total,
            updated: 0,
            unchanged: total,
            failed: 0,
        });
    }

    let base = data_dir()?;
    let mut items = lock(state.rule_sets(), "rule_set")?;
    let mut next = items.clone();
    let mut updated = 0;
    for (asset, bytes) in manifest.assets.iter().zip(assets.iter()) {
        let existing_index = next.iter().position(|item| item.id == asset.id);
        if existing_index.is_some_and(|index| !next[index].built_in) {
            return Err(AppError::invalid_argument(format!(
                "reserved built-in rule id is already used: {}",
                asset.id
            )));
        }
        if existing_index.is_some_and(|index| profile_is_current(&next[index], &manifest, asset)) {
            continue;
        }
        validate_asset(bytes, asset)?;
        let metadata = verify(bytes, VerifyMode::FullChecksum).map_err(|error| {
            AppError::internal(format!("invalid remote ZRS '{}': {error}", asset.id))
        })?;
        let artifact_path = install_artifact(&base, asset, bytes)?;
        let previous = existing_index.map(|index| next[index].clone());
        let profile = RuleSetProfile {
            id: asset.id.clone(),
            name: asset.name.clone(),
            enabled: previous.as_ref().map_or(true, |profile| profile.enabled),
            built_in: true,
            provenance: Some(RuleSetProvenance {
                repository: manifest.source_repository.clone(),
                revision: manifest.source_commit.clone(),
                license: manifest.source_license.clone(),
                source_url: asset.source_url.clone(),
                source_sha256: asset.source_sha256.clone(),
                ir_sha256: asset.ir_sha256.clone(),
            }),
            managed_by_subscription_id: None,
            common_binding: previous
                .as_ref()
                .and_then(|profile| profile.common_binding.clone())
                .or_else(|| {
                    Some(CommonRuleBinding {
                        enabled: true,
                        action: asset.default_action.clone(),
                        order: asset.default_order,
                    })
                }),
            semantic_ir: serde_json::json!({
                "version": 1,
                "name": asset.name,
                "rules": []
            }),
            source: None,
            source_state: RuleSetSourceState::default(),
            artifact: Some(ZrsArtifact {
                path: artifact_path.to_string_lossy().into_owned(),
                major_version: metadata.major_version,
                minor_version: metadata.minor_version,
                checksum: metadata.body_checksum,
                file_size: metadata.file_size,
                entry_count: metadata.entry_count(),
                built_at_unix_ms: manifest.generated_at_unix_ms,
            }),
            updated_at_unix_ms: manifest.generated_at_unix_ms,
            last_sync_at_unix_ms: Some(now_unix_ms()),
            last_error: None,
        };
        match existing_index {
            Some(index) => next[index] = profile,
            None => next.push(profile),
        }
        updated += 1;
    }
    if updated > 0 {
        domain_store::save_rule_sets(&next)?;
        *items = next;
    }
    Ok(RuleSetSyncAllOutcome {
        total,
        updated,
        unchanged: total.saturating_sub(updated),
        failed: 0,
    })
}

fn fetch_remote_bundle() -> AppResult<(Manifest, Vec<Vec<u8>>)> {
    let embedded: Manifest = serde_json::from_str(MANIFEST_JSON)
        .map_err(|error| AppError::internal(format!("invalid embedded rule manifest: {error}")))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AppError::internal(format!("failed to build rule client: {error}")))?;
    let manifest_bytes = download_limited(
        &client,
        REMOTE_MANIFEST_URL,
        MAX_REMOTE_MANIFEST_BYTES,
        "built-in rule manifest",
    )?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        AppError::invalid_argument(format!("invalid remote built-in rule manifest: {error}"))
    })?;
    validate_remote_manifest(&manifest, &embedded)?;
    let mut assets = Vec::with_capacity(manifest.assets.len());
    for asset in &manifest.assets {
        let url = format!("{REMOTE_ASSET_ROOT}/{}", asset.zrs_file);
        let bytes = download_limited(&client, &url, MAX_REMOTE_ASSET_BYTES, &asset.id)?;
        validate_asset(&bytes, asset)?;
        assets.push(bytes);
    }
    Ok((manifest, assets))
}

fn validate_remote_manifest(manifest: &Manifest, embedded: &Manifest) -> AppResult<()> {
    if manifest.schema != "znet.builtin-rules/v1" || manifest.version != 2 {
        return Err(AppError::invalid_argument(
            "unsupported remote built-in rule manifest",
        ));
    }
    if manifest.source_repository != embedded.source_repository
        || manifest.source_license != embedded.source_license
    {
        return Err(AppError::invalid_argument(
            "remote built-in rule provenance does not match the embedded bundle",
        ));
    }
    let expected = embedded
        .assets
        .iter()
        .map(|asset| (&asset.id, &asset.zrs_file))
        .collect::<std::collections::BTreeSet<_>>();
    let actual = manifest
        .assets
        .iter()
        .map(|asset| (&asset.id, &asset.zrs_file))
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected || actual.len() != manifest.assets.len() {
        return Err(AppError::invalid_argument(
            "remote built-in rule asset set does not match the client contract",
        ));
    }
    Ok(())
}

fn download_limited(
    client: &reqwest::blocking::Client,
    url: &str,
    limit: usize,
    label: &str,
) -> AppResult<Vec<u8>> {
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| AppError::internal(format!("failed to download {label}: {error}")))?;
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::invalid_argument(format!(
            "downloaded {label} exceeds the size limit"
        )));
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::internal(format!("failed to read {label}: {error}")))?;
    if bytes.len() > limit {
        return Err(AppError::invalid_argument(format!(
            "downloaded {label} exceeds the size limit"
        )));
    }
    Ok(bytes)
}

fn validate_asset(zrs: &[u8], asset: &ManifestAsset) -> AppResult<()> {
    let sha256 = format!("{:x}", Sha256::digest(zrs));
    if sha256 != asset.zrs_sha256 {
        return Err(AppError::internal(format!(
            "built-in ZRS SHA-256 mismatch: {}",
            asset.id
        )));
    }
    let metadata = verify(zrs, VerifyMode::FullChecksum).map_err(|error| {
        AppError::internal(format!("invalid built-in ZRS '{}': {error}", asset.id))
    })?;
    if metadata.body_checksum != asset.zrs_checksum
        || metadata.file_size != asset.zrs_file_size
        || metadata.entry_count() != asset.entry_count
    {
        return Err(AppError::internal(format!(
            "built-in ZRS metadata mismatch: {}",
            asset.id
        )));
    }
    Ok(())
}

fn install_artifact(base: &Path, asset: &ManifestAsset, bytes: &[u8]) -> AppResult<PathBuf> {
    let directory = base.join("rule-artifacts").join(&asset.id);
    fs::create_dir_all(&directory)
        .map_err(|error| io_error("create directory", &directory, error))?;
    let target = directory.join(format!("bundle-v1-{:08x}.zrs", asset.zrs_checksum));
    if fs::read(&target).is_ok_and(|current| current == bytes) {
        return Ok(target);
    }

    let temporary = directory.join(format!(".bundle-{}.tmp", std::process::id()));
    let publish = || -> AppResult<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|error| io_error("create temporary file", &temporary, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("write temporary file", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error("flush temporary file", &temporary, error))?;
        if target.exists() {
            fs::remove_file(&target)
                .map_err(|error| io_error("replace invalid artifact", &target, error))?;
        }
        fs::rename(&temporary, &target)
            .map_err(|error| io_error("publish artifact", &target, error))?;
        Ok(())
    };
    if let Err(error) = publish() {
        let _ = fs::remove_file(temporary);
        return Err(error);
    }
    Ok(target)
}

fn embedded_file(name: &str) -> AppResult<&'static [u8]> {
    match name {
        "builtin-private-ip.zrs" => Ok(include_bytes!(
            "../../resources/builtin-rules/builtin-private-ip.zrs"
        )),
        "builtin-cn-domain.zrs" => Ok(include_bytes!(
            "../../resources/builtin-rules/builtin-cn-domain.zrs"
        )),
        "builtin-cn-ip.zrs" => Ok(include_bytes!(
            "../../resources/builtin-rules/builtin-cn-ip.zrs"
        )),
        "builtin-gfw-domain.zrs" => Ok(include_bytes!(
            "../../resources/builtin-rules/builtin-gfw-domain.zrs"
        )),
        _ => Err(AppError::internal(format!(
            "built-in rule resource is not embedded: {name}"
        ))),
    }
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!(
            "failed to {action} built-in rule '{}': {error}",
            path.display()
        ),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_verified_defaults_and_preserves_user_binding_state() {
        let root = std::env::temp_dir().join(format!(
            "znet-builtin-rules-{}-{}",
            std::process::id(),
            crate::services::common::now_unix_ms()
        ));
        let mut items = Vec::new();

        assert!(install_defaults_in(&root, &mut items).unwrap());
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|item| item.built_in));
        assert!(items.iter().all(|item| item
            .artifact
            .as_ref()
            .is_some_and(|artifact| Path::new(&artifact.path).is_file())));
        let gfw = items
            .iter_mut()
            .find(|item| item.id == "builtin-gfw-domain")
            .unwrap();
        gfw.common_binding.as_mut().unwrap().enabled = false;
        domain_store::save_rule_sets_to_dir(&root, &items).unwrap();

        assert!(!install_defaults_in(&root, &mut items).unwrap());
        assert!(
            !items
                .iter()
                .find(|item| item.id == "builtin-gfw-domain")
                .unwrap()
                .common_binding
                .as_ref()
                .unwrap()
                .enabled
        );

        let artifact_path = items
            .iter()
            .find(|item| item.id == "builtin-gfw-domain")
            .unwrap()
            .artifact
            .as_ref()
            .unwrap()
            .path
            .clone();
        fs::write(&artifact_path, b"corrupt").unwrap();

        assert!(install_defaults_in(&root, &mut items).unwrap());
        assert!(verify(&fs::read(&artifact_path).unwrap(), VerifyMode::FullChecksum).is_ok());
        assert!(
            !items
                .iter()
                .find(|item| item.id == "builtin-gfw-domain")
                .unwrap()
                .common_binding
                .as_ref()
                .unwrap()
                .enabled
        );
        assert!(
            crate::models::app_config::AppConfig::default()
                .routing
                .inject_common_rules
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn embedded_manifest_and_assets_match_the_remote_update_contract() {
        let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();

        validate_remote_manifest(&manifest, &manifest).unwrap();
        for asset in &manifest.assets {
            validate_asset(embedded_file(&asset.zrs_file).unwrap(), asset).unwrap();
        }
    }

    #[test]
    fn remote_manifest_cannot_replace_or_duplicate_the_builtin_asset_set() {
        let embedded: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
        let mut missing = embedded.clone();
        missing.assets.pop();
        assert!(validate_remote_manifest(&missing, &embedded).is_err());

        let mut duplicate = embedded.clone();
        duplicate.assets.push(duplicate.assets[0].clone());
        assert!(validate_remote_manifest(&duplicate, &embedded).is_err());
    }

    #[test]
    fn asset_sha256_is_checked_before_publication() {
        let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
        let asset = &manifest.assets[0];
        let mut corrupted = embedded_file(&asset.zrs_file).unwrap().to_vec();
        corrupted[0] ^= 0xff;

        let error = validate_asset(&corrupted, asset).unwrap_err();
        assert!(error.message.contains("SHA-256 mismatch"));
    }
}
