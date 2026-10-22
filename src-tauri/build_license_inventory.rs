use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

// The mobile CLI forwards TAURI_* through its Xcode/Gradle options channel.
// Ordinary shell variables can disappear in Xcode's sanitized build process.
fn build_input(name: &str) -> Result<Option<String>, String> {
    let direct = env::var(name).ok();
    let forwarded = env::var(format!("TAURI_{name}")).ok();
    match (&direct, &forwarded) {
        (Some(a), Some(b)) if a != b => Err("conflicting-license-build-inputs".into()),
        _ => Ok(forwarded.or(direct)),
    }
}

/// Plain cargo builds need neither Node nor a nested dependency resolution/network step.
/// Actual release pipelines opt into strict verification of their precollected snapshot.
pub fn emit(selected_features: &[&str]) {
    println!("cargo:rerun-if-env-changed=ZNET_LICENSE_INVENTORY");
    println!("cargo:rerun-if-env-changed=ZNET_REQUIRE_LICENSE_INVENTORY");
    println!("cargo:rerun-if-env-changed=TAURI_ZNET_LICENSE_INVENTORY");
    println!("cargo:rerun-if-env-changed=TAURI_ZNET_REQUIRE_LICENSE_INVENTORY");
    let target = env::var("TARGET").expect("Cargo target");
    let profile = env::var("PROFILE").expect("Cargo profile");
    let output =
        Path::new(&env::var("OUT_DIR").expect("Cargo output")).join("license-inventory.rust.json");
    let strict = [
        "ZNET_REQUIRE_LICENSE_INVENTORY",
        "TAURI_ZNET_REQUIRE_LICENSE_INVENTORY",
    ]
    .iter()
    .any(|name| env::var(name).as_deref() == Ok("1"));
    let mut features: Vec<String> = selected_features.iter().map(|s| (*s).to_string()).collect();
    if env::var_os("CARGO_FEATURE_ACCEPTANCE_TEST_UTILS").is_some() {
        features.push("acceptance-test-utils".into());
    }
    features.sort();
    let unavailable = |reason: &str| {
        serde_json::json!({"schemaVersion":1,
        "target":{"platform":if target.contains("android"){"android"}else if target.contains("ios"){"ios"}else{"desktop"},"triple":target,"profile":profile,"features":features},
        "provenance":{"kind":"inventory-unavailable"},"entries":[],"texts":[],
        "gaps":[{"ecosystem":"cargo","reason":reason}]})
    };
    let result = (|| -> Result<serde_json::Value, String> {
        build_input("ZNET_REQUIRE_LICENSE_INVENTORY")?;
        let path = build_input("ZNET_LICENSE_INVENTORY")?
            .ok_or("precollected-target-inventory-not-provided")?;
        println!("cargo:rerun-if-changed={path}");
        let bytes = fs::read(&path).map_err(|_| "precollected-inventory-unreadable")?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("precollected-inventory-exceeds-budget".into());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "precollected-inventory-invalid-json")?;
        if value["schemaVersion"] != 1
            || value["target"]["triple"] != target
            || value["target"]["profile"] != profile
        {
            return Err("precollected-inventory-target-profile-mismatch".into());
        }
        let mut declared: Vec<String> = value["target"]["features"]
            .as_array()
            .ok_or("inventory-features-missing")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("inventory-feature-invalid")
            })
            .collect::<Result<_, _>>()?;
        declared.sort();
        if declared != features {
            return Err("precollected-inventory-feature-mismatch".into());
        }
        let inputs = value["provenance"]["inputs"]
            .as_array()
            .ok_or("inventory-input-provenance-missing")?;
        for (label, path) in [
            ("src-tauri/Cargo.toml", "Cargo.toml"),
            ("src-tauri/Cargo.lock", "Cargo.lock"),
        ] {
            println!("cargo:rerun-if-changed={path}");
            let expected = inputs
                .iter()
                .find(|v| v["label"] == label)
                .and_then(|v| v["sha256"].as_str())
                .ok_or("inventory-workspace-provenance-missing")?;
            let actual = format!(
                "{:x}",
                Sha256::digest(fs::read(path).map_err(|_| "workspace-manifest-unreadable")?)
            );
            if actual != expected {
                return Err("precollected-inventory-workspace-input-changed".into());
            }
        }
        // Reviewed originals affect the embedded text just as Cargo.lock affects
        // dependency identity. Never accept an inventory collected before an edit.
        let material_label = "resources/licenses/rust-package-materials.json";
        let material_path = Path::new("..").join(material_label);
        println!("cargo:rerun-if-changed={}", material_path.display());
        let material_input = inputs.iter().find(|v| v["label"] == material_label);
        if material_path.exists() || material_input.is_some() {
            let expected = material_input
                .and_then(|v| v["sha256"].as_str())
                .ok_or("inventory-package-material-provenance-missing")?;
            let actual = format!(
                "{:x}",
                Sha256::digest(
                    fs::read(material_path).map_err(|_| "package-material-cache-unreadable")?
                )
            );
            if actual != expected {
                return Err("precollected-inventory-package-material-changed".into());
            }
        }
        for input in inputs {
            let Some(label) = input["label"].as_str() else {
                continue;
            };
            if label != "LICENSE" && !label.starts_with("src-tauri/crates/") {
                continue;
            }
            if !Path::new(label)
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
            {
                return Err("inventory-local-input-path-invalid".into());
            }
            let path = Path::new("..").join(label);
            println!("cargo:rerun-if-changed={}", path.display());
            let expected = input["sha256"]
                .as_str()
                .ok_or("inventory-local-input-digest-missing")?;
            let actual = format!(
                "{:x}",
                Sha256::digest(fs::read(path).map_err(|_| "inventory-local-input-unreadable")?)
            );
            if expected != actual {
                return Err("precollected-inventory-local-input-changed".into());
            }
        }
        for text in value["texts"].as_array().ok_or("inventory-texts-missing")? {
            let content = text["text"].as_str().ok_or("inventory-text-invalid")?;
            let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
            if text["sha256"] != hash || text["id"] != format!("sha256-{hash}") {
                return Err("inventory-text-digest-mismatch".into());
            }
        }
        let material_ids: std::collections::BTreeSet<&str> = value["texts"]
            .as_array()
            .ok_or("inventory-texts-missing")?
            .iter()
            .map(|text| text["id"].as_str().ok_or("inventory-text-id-invalid"))
            .collect::<Result<_, _>>()?;
        let entries = value["entries"]
            .as_array()
            .ok_or("inventory-entries-missing")?;
        if entries.len() > 4096 || material_ids.len() > 8192 || !value["gaps"].is_array() {
            return Err("inventory-structure-budget-or-gaps-invalid".into());
        }
        for entry in entries {
            for field in ["licenseTextIds", "noticeTextIds"] {
                for id in entry[field]
                    .as_array()
                    .ok_or("inventory-material-references-missing")?
                {
                    if !id.as_str().is_some_and(|id| material_ids.contains(id)) {
                        return Err("inventory-material-reference-unresolved".into());
                    }
                }
            }
        }
        Ok(value)
    })();
    let value = match result {
        Ok(value) => value,
        Err(reason) if strict => panic!("required dependency license inventory rejected: {reason}"),
        Err(reason) => unavailable(&reason),
    };
    fs::write(
        output,
        serde_json::to_vec(&value).expect("serialize license inventory"),
    )
    .expect("write license inventory");
}
