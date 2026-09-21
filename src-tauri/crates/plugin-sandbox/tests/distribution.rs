#![recursion_limit = "256"]
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::SigningKey;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
use znet_plugin_sandbox::{
    contract::*,
    distribution::{
        directory::Directory,
        package,
        remote::{validate_download, ReleaseMetadata},
        store::Store,
    },
    policy::Authority,
    runtime::execute,
};
#[path = "support/distribution.rs"]
mod support;
use support::*;

fn application(version: &str) -> (package::ApplicationManifest, BTreeMap<String, Vec<u8>>) {
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":version,"requires_host":"=0.0.1","api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any",
        "required":[{"capability":"plugin.self.read","scope":"self"}],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()
    })).unwrap();
    let files = BTreeMap::from([
        (
            "components/identity/manifest.json".into(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        ),
        (
            "components/identity/index.js".into(),
            source.as_bytes().to_vec(),
        ),
        (
            "components/identity/lib/state.js".into(),
            b"export const state = {};".to_vec(),
        ),
        (
            "ui/manage/index.html".into(),
            b"<!doctype html><main>Manage</main>".to_vec(),
        ),
        (
            "ui/manage/style.css".into(),
            b"main { display: grid; }".to_vec(),
        ),
        ("assets/icon.svg".into(), b"<svg></svg>".to_vec()),
    ]);
    (
        package::ApplicationManifest {
            schema_version: 3,
            host: "znet-sink".into(),
            plugin_id: "org.example.plugin".into(),
            version: version.into(),
            components: vec![package::ApplicationComponent {
                id: "identity".into(),
                manifest: "components/identity/manifest.json".into(),
                entry: "components/identity/index.js".into(),
            }],
            pages: vec![package::ApplicationPage {
                id: "manage".into(),
                title: "Manage".into(),
                kind: package::PageKind::Management,
                entry: "ui/manage/index.html".into(),
            }],
            files: BTreeMap::new(),
        },
        files,
    )
}

fn rewrite_application(bytes: &[u8], path: &str, replacement: &[u8]) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let name = file.name().to_owned();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if name == path {
            data = replacement.to_vec();
        }
        entries.push((name, data));
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, data) in entries {
        writer.start_file(name, options).unwrap();
        writer.write_all(&data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn v3_application_package_preserves_modules_assets_and_embedded_identity() {
    let (manifest, files) = application("1.0.0");
    let mut reg = registration();
    reg.surfaces.push("znet-sink.ui.management.v1".into());
    let bytes =
        package::sign_application_with_registration(manifest, files, &SEED, reg.clone()).unwrap();
    assert!(bytes.starts_with(b"PK\x03\x04"));
    assert_eq!(package::package_id(&bytes).unwrap(), reg.id);
    assert_eq!(
        package::embedded_registration(&bytes).unwrap().unwrap().id,
        reg.id
    );
    let verified = package::verify(&bytes, &reg).unwrap();
    assert_eq!(verified.components.len(), 1);
    assert_eq!(verified.pages.len(), 1);
    assert_eq!(verified.pages[0].html, "<!doctype html><main>Manage</main>");
    assert_eq!(
        verified.resources["components/identity/lib/state.js"],
        b"export const state = {};"
    );
    assert_eq!(verified.resources["assets/icon.svg"], b"<svg></svg>");
}

#[test]
fn v3_application_rejects_tampered_unlisted_and_unsafe_entries() {
    let (manifest, files) = application("1.0.0");
    let reg = registration();
    let bytes =
        package::sign_application_with_registration(manifest, files, &SEED, reg.clone()).unwrap();
    let tampered = rewrite_application(
        &bytes,
        "components/identity/lib/state.js",
        b"export const state = {tampered:true};",
    );
    assert!(package::verify_local(&tampered, &reg).is_err());

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../plugin.json", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"{}").unwrap();
    writer
        .start_file("META-INF/signature.json", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"{}").unwrap();
    let unsafe_package = writer.finish().unwrap().into_inner();
    assert!(package::embedded_registration(&unsafe_package).is_err());

    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        writer
            .start_file(file.name(), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&data).unwrap();
    }
    writer
        .start_file("assets/unlisted.txt", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"not signed by the file index").unwrap();
    let unlisted = writer.finish().unwrap().into_inner();
    assert!(package::verify_local(&unlisted, &reg).is_err());
}

#[test]
fn v3_binary_packages_survive_install_restart_upgrade_and_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let mut reg = registration();
    reg.surfaces.push("znet-sink.ui.management.v1".into());
    let target = Target::native_desktop().unwrap();
    let first = {
        let (manifest, files) = application("1.0.0");
        package::sign_application_with_registration(manifest, files, &SEED, reg.clone()).unwrap()
    };
    let second = {
        let (manifest, files) = application("1.1.0");
        package::sign_application_with_registration(manifest, files, &SEED, reg.clone()).unwrap()
    };
    let third = {
        let (manifest, files) = application("1.2.0");
        package::sign_application_with_registration(manifest, files, &SEED, reg.clone()).unwrap()
    };
    let store = Store::new(temp.path()).unwrap();
    store.install(&first, &reg, &target, "0.0.1").unwrap();
    assert_eq!(
        Store::new(temp.path())
            .unwrap()
            .current(&reg)
            .unwrap()
            .version,
        "1.0.0"
    );
    store.install(&second, &reg, &target, "0.0.1").unwrap();
    assert_eq!(store.current(&reg).unwrap().version, "1.1.0");
    assert_eq!(
        store.rollback(&reg, &target, "0.0.1").unwrap().version,
        "1.0.0"
    );
    store.install(&third, &reg, &target, "0.0.1").unwrap();
    assert_eq!(store.current(&reg).unwrap().version, "1.2.0");
    let state = std::fs::read_to_string(temp.path().join("installed.json")).unwrap();
    assert!(state.contains("sha256"));
    assert!(!state.contains("znet-sink.plugin-package.v3"));
    assert_eq!(
        std::fs::read_dir(temp.path().join("packages"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn content_addressed_store_reads_and_upgrades_legacy_inline_state() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registration();
    let legacy = signed(&payload("1.0.0", serde_json::json!("any")));
    std::fs::write(
        temp.path().join("installed.json"),
        serde_json::to_vec(&serde_json::json!({
            "packages": {
                reg.id.clone(): {
                    "current": String::from_utf8(legacy).unwrap(),
                    "previous": null
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let store = Store::new(temp.path()).unwrap();
    assert_eq!(store.current(&reg).unwrap().version, "1.0.0");
    let next = signed(&payload("1.1.0", serde_json::json!("any")));
    let target = Target::native_desktop().unwrap();
    store.install(&next, &reg, &target, "0.0.1").unwrap();
    assert_eq!(store.current(&reg).unwrap().version, "1.1.0");
    assert_eq!(
        store.rollback(&reg, &target, "0.0.1").unwrap().version,
        "1.0.0"
    );
}
#[test]
fn any_devices_still_enforce_runtime_isolation_and_version() {
    let bytes = signed(&payload("1.0.0", serde_json::json!("any")));
    let package = package::verify(&bytes, &registration()).unwrap();
    let android = Target {
        os: Os::Android,
        arch: Arch::Aarch64,
        device: DeviceClass::Phone,
    };
    package.compatible(&android, "0.0.1").unwrap();
    assert!(package.compatible(&android, "2.0.0").is_err());
    let mut p = payload("1.0.0", serde_json::json!("any"));
    p.components[0].manifest.minimum_isolation = Isolation::Process;
    let package = package::verify(&signed(&p), &registration()).unwrap();
    assert_eq!(
        package.components[0].compatible(&android, "0.0.1", Isolation::Vm),
        Err(Error::UnsupportedIsolation)
    );
    for bad in [serde_json::json!("all"), serde_json::json!(null)] {
        let mut m = serde_json::to_value(&p.components[0].manifest).unwrap();
        m["targets"] = bad;
        assert!(
            Component::load(&serde_json::to_vec(&m).unwrap(), &p.components[0].source).is_err()
        );
    }
    let mut p = payload("1.0.0", serde_json::json!([]));
    assert!(package::verify(&signed(&p), &registration()).is_err());
    p.components[0].manifest.targets = Targets::Only(vec![Target {
        os: Os::Ios,
        arch: Arch::Aarch64,
        device: DeviceClass::Desktop,
    }]);
    assert!(package::verify(&signed(&p), &registration()).is_err());
}

#[test]
fn prerelease_clients_use_their_base_version_for_host_compatibility() {
    let android = Target {
        os: Os::Android,
        arch: Arch::Aarch64,
        device: DeviceClass::Phone,
    };
    let mut p = payload("1.0.0", serde_json::json!("any"));
    p.components[0].manifest.requires_host = ">=0.0.1, <0.1.0".into();
    let package = package::verify(&signed(&p), &registration()).unwrap();
    package
        .compatible(&android, "0.0.2-dev.202609151725")
        .unwrap();
    assert!(package
        .compatible(&android, "0.1.0-dev.202609151725")
        .is_err());

    p.components[0].manifest.requires_host = "=0.0.2-dev.202609151725".into();
    let package = package::verify(&signed(&p), &registration()).unwrap();
    package
        .compatible(&android, "0.0.2-dev.202609151725")
        .unwrap();
}
#[test]
fn publisher_signature_identity_capabilities_and_release_digest_are_checked() {
    let bytes = signed(&payload("1.0.0", serde_json::json!("any")));
    let reg = registration();
    package::verify(&bytes, &reg).unwrap();
    let mut other = reg.clone();
    other.publisher.public_key =
        STANDARD.encode(SigningKey::from_bytes(&[18; 32]).verifying_key().to_bytes());
    assert!(package::verify(&bytes, &other).is_err());
    other = reg.clone();
    other.id = "another.plugin".into();
    assert!(package::verify(&bytes, &other).is_err());
    other = reg.clone();
    other.capabilities.clear();
    assert!(package::verify(&bytes, &other).is_err());
    let mut envelope: package::Envelope = serde_json::from_slice(&bytes).unwrap();
    envelope.payload = STANDARD.encode(b"{}");
    assert!(package::verify(&serde_json::to_vec(&envelope).unwrap(), &reg).is_err());
    let mut metadata = ReleaseMetadata {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: reg.id.clone(),
        version: "1.0.0".into(),
        asset: "plugin.zspkg".into(),
        sha256: sha256(&bytes),
    };
    validate_download(&bytes, &metadata, &reg).unwrap();
    metadata.version = "2.0.0".into();
    assert!(validate_download(&bytes, &metadata, &reg).is_err());
    metadata.version = "1.0.0".into();
    metadata.sha256 = "00".repeat(32);
    assert!(validate_download(&bytes, &metadata, &reg).is_err());
}

#[test]
fn signed_management_pages_require_a_registered_surface_and_are_verified() {
    let mut p = payload("1.0.0", serde_json::json!("any"));
    p.pages.push(package::SourcePage {
        id: "manage".into(),
        title: "Manage".into(),
        kind: package::PageKind::Management,
        html: "<!doctype html><button>Sign in</button>".into(),
    });
    let bytes = signed(&p);
    assert!(package::verify(&bytes, &registration()).is_err());

    let mut reg = registration();
    reg.surfaces.push("znet-sink.ui.management.v1".into());
    let verified = package::verify(&bytes, &reg).unwrap();
    assert_eq!(verified.pages.len(), 1);
    assert_eq!(verified.pages[0].id, "manage");
    assert_eq!(verified.pages[0].kind, package::PageKind::Management);

    p.pages.push(p.pages[0].clone());
    assert!(package::verify(&signed(&p), &reg).is_err());
}

#[test]
fn self_contained_local_package_is_signed_and_does_not_require_marketplace_surface_admission() {
    let mut p = payload("1.0.0", serde_json::json!("any"));
    p.pages.push(package::SourcePage {
        id: "manage".into(),
        title: "Manage".into(),
        kind: package::PageKind::Management,
        html: "<!doctype html><button>Sign in</button>".into(),
    });
    let registration = registration();
    let bytes = package::sign_with_registration(
        &serde_json::to_vec(&p).unwrap(),
        &SEED,
        registration.clone(),
    )
    .unwrap();
    let embedded = package::embedded_registration(&bytes).unwrap().unwrap();
    assert_eq!(
        embedded.publisher.public_key,
        registration.publisher.public_key
    );
    assert!(package::verify(&bytes, &registration).is_err());
    assert_eq!(
        package::verify_local(&bytes, &embedded)
            .unwrap()
            .pages
            .len(),
        1
    );

    let mut envelope: package::Envelope = serde_json::from_slice(&bytes).unwrap();
    envelope.registration.as_mut().unwrap().name = "Tampered".into();
    let tampered = serde_json::to_vec(&envelope).unwrap();
    let embedded = package::embedded_registration(&tampered).unwrap().unwrap();
    assert!(package::verify_local(&tampered, &embedded).is_err());
}
#[test]
fn registered_install_upgrade_restart_rollback_remove_and_explicit_execution() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registration();
    let target = Target::native_desktop().unwrap();
    let store = Store::new(temp.path()).unwrap();
    let first = signed(&payload("1.0.0", serde_json::json!("any")));
    let package = store.install(&first, &reg, &target, "0.0.1").unwrap();
    let component = &package.components[0];
    let grants = BTreeSet::from([Request {
        capability: Capability::SelfRead,
        scope: "self".into(),
    }]);
    let auth = Authority::admit(component, &grants).unwrap();
    assert_eq!(
        execute(component, &auth, None, Arc::new(AtomicBool::new(false))),
        Err(Error::Disabled)
    );
    auth.authorize(grants, Duration::from_secs(5)).unwrap();
    assert_eq!(
        execute(component, &auth, None, Arc::new(AtomicBool::new(false))).unwrap()["plugin_id"],
        reg.id
    );
    let second = signed(&payload("1.1.0", serde_json::json!("any")));
    let upgraded = store.install(&second, &reg, &target, "0.0.1").unwrap();
    assert!(matches!(
        auth.begin(&upgraded.components[0]),
        Err(Error::DigestMismatch)
    ));
    assert!(store.install(&first, &reg, &target, "0.0.1").is_err());
    let reopened = Store::new(temp.path()).unwrap();
    assert_eq!(reopened.current(&reg).unwrap().version, "1.1.0");
    assert_eq!(
        reopened.rollback(&reg, &target, "0.0.1").unwrap().version,
        "1.0.0"
    );
    reopened.uninstall(&reg.id).unwrap();
    assert!(reopened.list().unwrap().is_empty());
}
#[test]
fn failed_install_leaves_previous_state_and_catalog_withdrawal_blocks_lookup() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(temp.path()).unwrap();
    let reg = registration();
    let target = Target::native_desktop().unwrap();
    store
        .install(
            &signed(&payload("1.0.0", serde_json::json!("any"))),
            &reg,
            &target,
            "0.0.1",
        )
        .unwrap();
    let unsupported = payload(
        "1.1.0",
        serde_json::json!([{"os":"android","arch":"aarch64","device":"phone"}]),
    );
    assert!(store
        .install(&signed(&unsupported), &reg, &target, "0.0.1")
        .is_err());
    assert!(store.install(b"invalid", &reg, &target, "0.0.1").is_err());
    assert_eq!(store.current(&reg).unwrap().version, "1.0.0");
    let directory =
        Directory::parse(br#"{"schema_version":2,"host":"znet-sink","plugins":[]}"#).unwrap();
    assert!(directory.find(&reg.id).is_err());
}
#[test]
fn duplicate_registration_wrong_host_and_untrusted_repository_rejected() {
    let reg = registration();
    let directory = serde_json::json!({"schema_version":2,"host":"znet-sink","plugins":[reg.clone(),reg.clone()]});
    assert!(Directory::parse(&serde_json::to_vec(&directory).unwrap()).is_err());
    assert!(Directory::parse(br#"{"schema_version":2,"host":"zboard","plugins":[]}"#).is_err());
    for url in [
        "http://github.com/a/b",
        "https://evil.com/a/b",
        "https://github.com/a/b/c",
        "https://user@github.com/a/b",
    ] {
        let mut bad = reg.clone();
        bad.repository = url.into();
        assert!(bad.validate().is_err());
    }
}

#[test]
fn tampered_installed_store_is_not_trusted() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(temp.path()).unwrap();
    let reg = registration();
    store
        .install(
            &signed(&payload("1.0.0", serde_json::json!("any"))),
            &reg,
            &Target::native_desktop().unwrap(),
            "0.0.1",
        )
        .unwrap();
    let path = temp.path().join("installed.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["packages"][&reg.id]["current"] = serde_json::json!("{}");
    std::fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(store.current(&reg).is_err());
}

#[test]
fn author_cli_emits_verifiable_package_and_release_metadata_without_private_key() {
    let temp = tempfile::tempdir().unwrap();
    let p = payload("1.0.0", serde_json::json!("any"));
    let manifest = temp.path().join("manifest.json");
    let source = temp.path().join("main.js");
    let seed = temp.path().join("private.seed");
    let out = temp.path().join("plugin.zspkg");
    let metadata = temp.path().join("marketplace-entry.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&p.components[0].manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(&source, &p.components[0].source).unwrap();
    std::fs::write(&seed, SEED).unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_znet-plugin"))
            .arg("pack")
            .args([&manifest, &source, &seed, &out, &metadata])
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&STANDARD.encode(SEED)));
    let bytes = std::fs::read(&out).unwrap();
    let meta: ReleaseMetadata = serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    validate_download(&bytes, &meta, &registration()).unwrap();
    assert!(!run().status.success());
    assert_eq!(std::fs::read(out).unwrap(), bytes);
}

#[test]
fn author_cli_packs_a_v3_application_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    std::fs::create_dir_all(root.join("components/identity/lib")).unwrap();
    std::fs::create_dir_all(root.join("ui/manage")).unwrap();
    let (manifest, files) = application("1.0.0");
    std::fs::write(
        root.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let seed = temp.path().join("private.seed");
    let registration_path = temp.path().join("registration.json");
    let output = temp.path().join("application.zspkg");
    let metadata = temp.path().join("marketplace-entry.json");
    std::fs::write(&seed, SEED).unwrap();
    let mut reg = registration();
    reg.surfaces.push("znet-sink.ui.management.v1".into());
    std::fs::write(&registration_path, serde_json::to_vec_pretty(&reg).unwrap()).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_znet-plugin"))
        .arg("pack-app")
        .args([&root, &seed, &output, &metadata, &registration_path])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&output).unwrap();
    let verified = package::verify(&bytes, &reg).unwrap();
    assert_eq!(verified.version, "1.0.0");
    assert!(verified
        .resources
        .contains_key("components/identity/lib/state.js"));
    let release: ReleaseMetadata =
        serde_json::from_slice(&std::fs::read(metadata).unwrap()).unwrap();
    validate_download(&bytes, &release, &reg).unwrap();

    let unsafe_seed = root.join("private.seed");
    std::fs::write(&unsafe_seed, SEED).unwrap();
    let rejected = std::process::Command::new(env!("CARGO_BIN_EXE_znet-plugin"))
        .arg("pack-app")
        .args([
            &root,
            &unsafe_seed,
            &temp.path().join("unsafe.zspkg"),
            &temp.path().join("unsafe.json"),
            &registration_path,
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
}
#[cfg(unix)]
#[test]
fn store_rejects_symlink_state_without_touching_target() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    std::fs::write(&outside, b"keep").unwrap();
    let root = temp.path().join("store");
    let store = Store::new(&root).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("installed.json")).unwrap();
    assert!(store.list().is_err());
    assert_eq!(std::fs::read(outside).unwrap(), b"keep");
}
