#![recursion_limit = "256"]
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::SigningKey;
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
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
