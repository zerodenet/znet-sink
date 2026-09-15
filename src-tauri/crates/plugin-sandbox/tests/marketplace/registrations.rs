use super::*;

pub(super) fn snapshot() -> Value {
    let mut product = page("1.0.0", "stable").0["items"][0].clone();
    product["release_source"] =
        json!({"type":"github-releases","metadata_asset":"marketplace-entry.json"});
    product["targets"][0]["releases"] = json!([]);
    let mut server = product.clone();
    server["id"] = json!("other.product");
    server["targets"][0]["host"] = json!("zboard");
    json!({"schema_version":1, "snapshot_version":format!("sha256:{}", "a".repeat(64)),
        "sources":{"registry_schema":3,"stale_products":[]}, "products":[product,server]})
}

#[test]
fn unpublished_registration_remains_available_for_local_install_and_existing_packages() {
    let remote = Remote::with_fetch(|url, _| {
        assert_eq!(url, "https://plugins.zerodenet.org/api/plugins.json");
        Ok(serde_json::to_vec(&snapshot()).unwrap())
    })
    .unwrap();
    let directory = remote.directory().unwrap();
    assert_eq!(directory.plugins.len(), 1);
    let registration = directory.find("org.example.plugin").unwrap();
    assert!(serde_json::to_value(registration)
        .unwrap()
        .get("releases")
        .is_none());
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path()).unwrap();
    let bytes = signed(&payload("1.0.0", json!("any")));
    store
        .install(
            &bytes,
            registration,
            &Target::native_desktop().unwrap(),
            "0.0.1",
        )
        .unwrap();
    assert_eq!(store.current(registration).unwrap().version, "1.0.0");
    assert!(store
        .current(directory.find("org.example.plugin").unwrap())
        .is_ok());
}

#[test]
fn withdrawn_and_empty_registrations_never_fall_back_to_a_previous_listing() {
    for empty in [false, true] {
        let mut snapshot = snapshot();
        if empty {
            snapshot["products"] = json!([]);
        } else {
            snapshot["products"][0]["withdrawn"] = json!(true);
        }
        let remote = Remote::with_fetch(|url, _| {
            assert_ne!(url, DIRECTORY_URL);
            Ok(serde_json::to_vec(&snapshot).unwrap())
        })
        .unwrap();
        assert!(remote.directory().unwrap().plugins.is_empty());
    }
}

#[test]
fn registration_endpoint_outage_uses_the_generated_catalog() {
    let remote = Remote::with_fetch(|url, _| {
        if url != DIRECTORY_URL {
            return Err("unavailable".into());
        }
        Ok(serde_json::to_vec(
            &json!({"schema_version":2,"host":"znet-sink","plugins":[registration()]}),
        )
        .unwrap())
    })
    .unwrap();
    assert_eq!(remote.directory().unwrap().plugins.len(), 1);
}

#[test]
fn malformed_registration_identity_and_duplicate_packages_are_rejected() {
    use znet_plugin_sandbox::distribution::directory::Directory;
    for (pointer, value) in [
        ("/schema_version", json!(2)),
        ("/sources/registry_schema", json!(1)),
        ("/snapshot_version", json!("invalid")),
        ("/products/0/id", json!("invalid product")),
        (
            "/products/0/repository",
            json!("https://github.com/other/repo/extra"),
        ),
        ("/products/0/targets/0/host", json!("unknown")),
        ("/products/0/publisher/public_key", json!("invalid")),
    ] {
        let mut bad = snapshot();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            Directory::parse_marketplace_registrations(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "accepted {pointer}"
        );
    }
    let mut duplicate = snapshot();
    let mut second = duplicate["products"][0].clone();
    second["id"] = json!("other.product");
    duplicate["products"][1] = second;
    assert!(
        Directory::parse_marketplace_registrations(&serde_json::to_vec(&duplicate).unwrap())
            .is_err()
    );
}
