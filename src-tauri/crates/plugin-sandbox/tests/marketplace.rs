use serde_json::{json, Value};
use std::cell::RefCell;
use znet_plugin_sandbox::{
    contract::{sha256, Target},
    distribution::{
        package,
        remote::{Remote, DIRECTORY_URL},
        store::Store,
    },
};
#[path = "support/distribution.rs"]
mod support;
use support::*;

fn page(version: &str, channel: &str) -> (Value, Vec<u8>) {
    let bytes = signed(&payload(version, json!("any")));
    let envelope: package::Envelope = serde_json::from_slice(&bytes).unwrap();
    let reg = registration();
    let artifact = json!({"os":"any", "arch":"any",
        "url":format!("https://github.com/example/plugin/releases/download/v{version}/plugin.zspkg"),
        "size":bytes.len(), "sha256":sha256(&bytes),
        "signature":{"algorithm":"ed25519","value":envelope.signature}});
    let release = json!({"version":version,"channel":channel,"published_at":"2026-09-15T00:00:00Z",
        "notes_url":format!("https://github.com/example/plugin/releases/tag/v{version}"),
        "host_version":{"min":"0.0.1","max_exclusive":"0.1.0"},
        "surfaces":[],"capabilities":["plugin.self.read"],"artifacts":[artifact]});
    let product = json!({"id":"example.product", "repository":reg.repository,"publisher":reg.publisher,
        "name":"发布者原始名称", "description":"发布者原始简介", "license":"MIT", "maintainers":["example"],
        "targets":[{"host":"znet-sink","package_id":reg.id,"surfaces":[],"capabilities":reg.capabilities,"releases":[release]}]});
    (
        json!({"snapshot_version":format!("sha256:{}", "a".repeat(64)),"generated_at":"2026-09-15T00:00:00Z",
        "page":1,"page_size":1000,"total":1,"items":[product]}),
        bytes,
    )
}
fn manifest(page: &Value) -> Value {
    let product = &page["items"][0];
    let target = &product["targets"][0];
    let record = &target["releases"][0];
    json!({"schema_version":1,"product_id":product["id"],
        "repository":product["repository"],"publisher":product["publisher"],
        "source":{"tag":format!("v{}",record["version"].as_str().unwrap()),"commit":"a".repeat(40)},
        "release":{"version":record["version"],"channel":record["channel"],
            "published_at":record["published_at"],"notes_url":record["notes_url"],
            "targets":[{"host":"znet-sink","package_id":target["package_id"],
                "host_version":record["host_version"],"surfaces":record["surfaces"],
                "capabilities":record["capabilities"],"artifacts":record["artifacts"]}]}})
}
fn github(page: &Value) -> znet_plugin_sandbox::distribution::remote::Release {
    let record = &page["items"][0]["targets"][0]["releases"][0];
    let version = record["version"].as_str().unwrap();
    let mut assets = vec![json!({"name":"marketplace-entry.json", "size":1,
        "browser_download_url":format!("https://github.com/example/plugin/releases/download/v{version}/marketplace-entry.json")})];
    for artifact in record["artifacts"].as_array().unwrap() {
        assets.push(json!({"name":"plugin.zspkg", "size":artifact["size"], "browser_download_url":artifact["url"]}));
    }
    serde_json::from_value(json!({"tag_name":format!("v{version}"),"draft":false,
        "prerelease":record["channel"] != "stable", "published_at":record["published_at"],
        "html_url":record["notes_url"],"assets":assets}))
    .unwrap()
}
fn download(page: &Value, bytes: &[u8]) -> znet_plugin_sandbox::distribution::Result<Vec<u8>> {
    let document = manifest(page);
    let remote = Remote::with_fetch(|url, _| {
        Ok(if url.ends_with("marketplace-entry.json") {
            serde_json::to_vec(&document).unwrap()
        } else {
            bytes.to_vec()
        })
    })
    .unwrap();
    remote.download(&registration(), &github(page))
}
fn record(page: &mut Value) -> &mut Value {
    &mut page["items"][0]["targets"][0]["releases"][0]
}
fn artifact(page: &mut Value) -> &mut Value {
    &mut record(page)["artifacts"][0]
}

#[test]
fn discovery_ignores_central_release_feeds_and_queries_registered_repository() {
    let seen = RefCell::new(Vec::new());
    let mut central = registrations::snapshot();
    // Central release data is deliberately stale and malformed. It is not an install source.
    central["products"][0]["targets"][0]["releases"] = json!([{"version":"old"}]);
    let current = github(&page("1.2.0", "stable").0);
    let remote = Remote::with_fetch(|url, _| {
        seen.borrow_mut().push(url.to_owned());
        if url == "https://plugins.zerodenet.org/api/plugins.json" {
            return Ok(serde_json::to_vec(&central).unwrap());
        }
        if url == "https://api.github.com/repos/example/plugin/releases?per_page=100" {
            let mut draft = current.clone();
            draft.draft = true;
            let mut unpublished = current.clone();
            unpublished.published_at = None;
            return Ok(serde_json::to_vec(&vec![current.clone(), draft, unpublished]).unwrap());
        }
        assert_eq!(
            url,
            "https://api.github.com/repos/example/plugin/releases/tags/v1.2.0"
        );
        Ok(serde_json::to_vec(&current).unwrap())
    })
    .unwrap();
    let directory = remote.directory().unwrap();
    let reg = directory.find("org.example.plugin").unwrap();
    assert_eq!(reg.product_id.as_deref(), Some("example.product"));
    assert_eq!(reg.name, "发布者原始名称");
    let releases = remote.releases(reg).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].tag_name, "v1.2.0");
    assert_eq!(remote.release(reg, "v1.2.0").unwrap().tag_name, "v1.2.0");
    assert_eq!(seen.borrow().len(), 3);
}

#[test]
fn repository_failure_is_reported_without_using_a_central_cached_release() {
    let remote = Remote::with_fetch(|url, _| {
        if url == "https://plugins.zerodenet.org/api/plugins.json" {
            return Ok(serde_json::to_vec(&registrations::snapshot()).unwrap());
        }
        assert!(url.starts_with("https://api.github.com/repos/example/plugin/releases"));
        Err("repository unavailable".into())
    })
    .unwrap();
    let directory = remote.directory().unwrap();
    let reg = &directory.plugins[0];
    assert!(remote.releases(reg).is_err());
    assert!(remote.release(reg, "v1.0.0").is_err());
}

#[test]
fn malformed_publisher_manifest_platform_range_and_boundary_are_rejected() {
    let (original, bytes) = page("1.0.0", "stable");
    for (pointer, value) in [
        ("/items/0/id", json!("invalid product")),
        ("/items/0/targets/0/releases/0/channel", json!("nightly")),
        (
            "/items/0/targets/0/releases/0/host_version/max_exclusive",
            json!("0.0.1"),
        ),
        (
            "/items/0/targets/0/releases/0/capabilities",
            json!(["network.get"]),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/os",
            json!("macos"),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/arch",
            json!("x86_64"),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/url",
            json!("https://github.com/other/plugin/releases/download/v1.0.0/plugin.zspkg"),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/url",
            json!("https://user@github.com/example/plugin/releases/download/v1.0.0/plugin.zspkg"),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/url",
            json!("https://github.com/example/plugin/releases/download/v2.0.0/plugin.zspkg"),
        ),
        (
            "/items/0/targets/0/releases/0/artifacts/0/signature/algorithm",
            json!("none"),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(download(&bad, &bytes).is_err(), "accepted {pointer}");
    }
    let mut oversized = original.clone();
    artifact(&mut oversized)["size"] = json!(4 * 1024 * 1024 + 1);
    assert!(download(&oversized, &bytes).is_err());
    let mut ambiguous = original;
    let mut extra = artifact(&mut ambiguous).clone();
    let (os, arch) = if cfg!(target_os = "macos") {
        (
            "darwin",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "amd64"
            },
        )
    } else if cfg!(target_os = "windows") {
        (
            "windows",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "amd64"
            },
        )
    } else {
        (
            "linux",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "amd64"
            },
        )
    };
    extra["os"] = json!(os);
    extra["arch"] = json!(arch);
    record(&mut ambiguous)["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    assert!(download(&ambiguous, &bytes).is_err());
}

#[test]
fn repository_install_upgrade_and_restart_preserve_existing_store() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path()).unwrap();
    std::fs::write(root.path().join("user-data"), b"existing state").unwrap();
    for version in ["1.0.0", "1.1.0"] {
        let (page, bytes) = page(version, "stable");
        let reg = &registration();
        let downloaded = download(&page, &bytes).unwrap();
        store
            .install(
                &downloaded,
                reg,
                &Target::native_desktop().unwrap(),
                "0.0.1",
            )
            .unwrap();
        assert_eq!(
            Store::new(root.path())
                .unwrap()
                .current(reg)
                .unwrap()
                .version,
            version
        );
    }
    assert_eq!(
        std::fs::read(root.path().join("user-data")).unwrap(),
        b"existing state"
    );
    assert_eq!(
        store
            .rollback(&registration(), &Target::native_desktop().unwrap(), "0.0.1")
            .unwrap()
            .version,
        "1.0.0"
    );
}

#[test]
fn package_bytes_must_match_size_digest_signature_identity_and_release_capabilities() {
    let (original, bytes) = page("1.0.0", "stable");
    for fault in [
        "size",
        "digest",
        "signature",
        "capability",
        "publisher",
        "version",
    ] {
        let mut bad = original.clone();
        match fault {
            "size" => artifact(&mut bad)["size"] = json!(bytes.len() + 1),
            "digest" => artifact(&mut bad)["sha256"] = json!("00".repeat(32)),
            "signature" => {
                artifact(&mut bad)["signature"]["value"] = json!(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    [0; 64]
                ))
            }
            "capability" => record(&mut bad)["capabilities"] = json!([]),
            "publisher" => {
                bad["items"][0]["publisher"]["public_key"] = json!(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    ed25519_dalek::SigningKey::from_bytes(&[18; 32])
                        .verifying_key()
                        .to_bytes()
                ))
            }
            "version" => {
                record(&mut bad)["version"] = json!("1.1.0");
                artifact(&mut bad)["url"] = json!(
                    "https://github.com/example/plugin/releases/download/v1.1.0/plugin.zspkg"
                );
            }
            _ => unreachable!(),
        }
        assert!(download(&bad, &bytes).is_err(), "accepted {fault}");
    }
}

#[path = "marketplace/registrations.rs"]
mod registrations;
