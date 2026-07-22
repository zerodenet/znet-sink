use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use zero_rule::zrs::{verify, VerifyMode};

use super::{data_dir, domain_store};
use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{
    CommonRuleAction, CommonRuleBinding, RuleSetProfile, RuleSetProvenance, RuleSetSourceState,
    ZrsArtifact,
};

const MANIFEST_JSON: &str = include_str!("../../resources/builtin-rules/manifest.json");

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAsset {
    id: String,
    name: String,
    zrs_file: String,
    source_url: String,
    source_sha256: String,
    ir_sha256: String,
    zrs_checksum: u32,
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
    if manifest.schema != "znet.builtin-rules/v1" || manifest.version != 1 {
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
    fs::read(path)
        .ok()
        .and_then(|bytes| verify(&bytes, VerifyMode::FullChecksum).ok())
        .is_some_and(|metadata| {
            metadata.body_checksum == asset.zrs_checksum
                && metadata.file_size == asset.zrs_file_size
                && metadata.entry_count() == asset.entry_count
        })
}

fn validate_asset(zrs: &[u8], asset: &ManifestAsset) -> AppResult<()> {
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
}
