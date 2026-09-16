use base64::Engine;
use serde_json::{json, Value};
use std::cell::RefCell;
use znet_plugin_sandbox::{
    contract::{sha256, Target},
    distribution::{
        directory::Directory,
        package,
        remote::{Remote, MARKETPLACE_API_URL},
        store::Store,
    },
};

#[path = "support/distribution.rs"]
mod support;
use support::*;

fn snapshot(version: &str, channel: &str, bytes: &[u8]) -> Value {
    let reg = registration();
    let envelope: package::Envelope = serde_json::from_slice(bytes).unwrap();
    json!({
        "schema_version": 1,
        "snapshot_version": format!("sha256:{}", "a".repeat(64)),
        "sources": {"registry_schema": 3, "stale_products": []},
        "products": [{
            "id": "example.product",
            "repository": reg.repository,
            "publisher": reg.publisher,
            "name": reg.name,
            "description": reg.description,
            "license": reg.license,
            "maintainers": reg.maintainers,
            "release_source": reg.release_source,
            "targets": [{
                "host": "znet-sink",
                "package_id": reg.id,
                "surfaces": reg.surfaces,
                "capabilities": reg.capabilities,
                "releases": [{
                    "version": version,
                    "channel": channel,
                    "published_at": "2026-09-15T00:00:00Z",
                    "notes_url": format!("https://github.com/example/plugin/releases/tag/v{version}"),
                    "host_version": {"min": "0.0.1", "max_exclusive": "0.1.0"},
                    "surfaces": [],
                    "capabilities": ["plugin.self.read"],
                    "artifacts": [{
                        "os": "any",
                        "arch": "any",
                        "url": format!("https://github.com/example/plugin/releases/download/v{version}/plugin.zspkg"),
                        "size": bytes.len(),
                        "sha256": sha256(bytes),
                        "signature": {"algorithm": "ed25519", "value": envelope.signature}
                    }]
                }]
            }]
        }]
    })
}

fn directory(value: &Value, host_version: &str) -> Directory {
    Directory::parse_marketplace(&serde_json::to_vec(value).unwrap(), host_version).unwrap()
}

#[test]
fn marketplace_metadata_selects_the_release_and_only_github_supplies_package_bytes() {
    let bytes = signed(&payload("1.0.0", json!("any")));
    let value = snapshot("1.0.0", "stable", &bytes);
    let directory = directory(&value, "0.0.1");
    let registration = &directory.plugins[0];
    assert_eq!(registration.product_id.as_deref(), Some("example.product"));
    assert_eq!(registration.releases.len(), 1);

    let seen = RefCell::new(Vec::new());
    let remote = Remote::with_fetch(|url, _| {
        panic!("release discovery must not query the publisher: {url}")
    })
    .unwrap()
    .with_package_fetch(|url, limit, expected_sha256| {
        seen.borrow_mut()
            .push((url.to_owned(), limit, expected_sha256.to_owned()));
        Ok(bytes.clone())
    });
    let releases = remote.releases(registration).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].tag_name, "v1.0.0");
    let selected = remote.release(registration, "v1.0.0").unwrap();
    assert_eq!(remote.download(registration, &selected).unwrap(), bytes);
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].0,
        "https://github.com/example/plugin/releases/download/v1.0.0/plugin.zspkg"
    );
    assert_eq!(seen[0].1, bytes.len());
    assert_eq!(seen[0].2, sha256(&bytes));
}

#[test]
fn current_marketplace_endpoint_is_metadata_only_and_future_domain_is_not_required() {
    assert_eq!(
        MARKETPLACE_API_URL,
        "https://zerodenet.github.io/plugins/api/plugins.json"
    );
    let remote = Remote::with_fetch(|url, _| {
        assert_eq!(url, MARKETPLACE_API_URL);
        Ok(serde_json::to_vec(&snapshot(
            "1.0.0",
            "stable",
            &signed(&payload("1.0.0", json!("any"))),
        ))
        .unwrap())
    })
    .unwrap()
    .for_host("0.0.1")
    .unwrap();
    assert_eq!(remote.directory().unwrap().plugins.len(), 1);
}

#[test]
fn incompatible_and_foreign_release_metadata_is_rejected_before_download() {
    let bytes = signed(&payload("1.0.0", json!("any")));
    let original = snapshot("1.0.0", "stable", &bytes);
    let mut incompatible = original.clone();
    incompatible["products"][0]["targets"][0]["releases"][0]["host_version"]["min"] =
        json!("1.0.0");
    incompatible["products"][0]["targets"][0]["releases"][0]["host_version"]
        .as_object_mut()
        .unwrap()
        .remove("max_exclusive");
    assert!(directory(&incompatible, "0.0.1").plugins[0]
        .releases
        .is_empty());

    for (pointer, replacement) in [
        ("/products/0/targets/0/releases/0/channel", json!("nightly")),
        (
            "/products/0/targets/0/releases/0/artifacts/0/url",
            json!("https://github.com/other/plugin/releases/download/v1.0.0/plugin.zspkg"),
        ),
        (
            "/products/0/targets/0/releases/0/artifacts/0/url",
            json!("https://plugins.zerodenet.org/download/plugin.zspkg"),
        ),
        (
            "/products/0/targets/0/releases/0/artifacts/0/signature/algorithm",
            json!("none"),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            Directory::parse_marketplace(&serde_json::to_vec(&bad).unwrap(), "0.0.1").is_err(),
            "accepted {pointer}"
        );
    }
}

#[test]
fn marketplace_ranges_treat_prerelease_clients_as_their_base_release() {
    let bytes = signed(&payload("1.0.0", json!("any")));
    let value = snapshot("1.0.0", "stable", &bytes);
    assert_eq!(
        directory(&value, "0.0.2-dev.202609151725").plugins[0]
            .releases
            .len(),
        1
    );
    assert!(directory(&value, "0.1.0-dev.202609151725").plugins[0]
        .releases
        .is_empty());
}

#[test]
fn package_size_digest_signature_and_identity_must_match_the_marketplace() {
    let bytes = signed(&payload("1.0.0", json!("any")));
    for fault in ["size", "digest", "signature", "version"] {
        let mut value = snapshot("1.0.0", "stable", &bytes);
        let artifact = &mut value["products"][0]["targets"][0]["releases"][0]["artifacts"][0];
        match fault {
            "size" => artifact["size"] = json!(bytes.len() + 1),
            "digest" => artifact["sha256"] = json!("0".repeat(64)),
            "signature" => {
                artifact["signature"]["value"] =
                    json!(base64::engine::general_purpose::STANDARD.encode([0_u8; 64]))
            }
            "version" => {
                value["products"][0]["targets"][0]["releases"][0]["version"] = json!("1.0.1")
            }
            _ => unreachable!(),
        }
        let Ok(directory) =
            Directory::parse_marketplace(&serde_json::to_vec(&value).unwrap(), "0.0.1")
        else {
            continue;
        };
        let registration = &directory.plugins[0];
        let remote = Remote::new()
            .unwrap()
            .with_package_fetch(|_, _, _| Ok(bytes.clone()));
        let release = remote.releases(registration).unwrap().remove(0);
        assert!(
            remote.download(registration, &release).is_err(),
            "accepted {fault}"
        );
    }
}

#[test]
fn verified_install_upgrade_preserves_store_and_rollback() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path()).unwrap();
    let mut registration = registration();
    for version in ["1.0.0", "1.1.0"] {
        let bytes = signed(&payload(version, json!("any")));
        registration.releases = directory(&snapshot(version, "stable", &bytes), "0.0.1")
            .plugins
            .remove(0)
            .releases;
        store
            .install(
                &bytes,
                &registration,
                &Target::native_desktop().unwrap(),
                "0.0.1",
            )
            .unwrap();
    }
    assert_eq!(store.current(&registration).unwrap().version, "1.1.0");
    assert_eq!(
        store
            .rollback(&registration, &Target::native_desktop().unwrap(), "0.0.1")
            .unwrap()
            .version,
        "1.0.0"
    );
}
