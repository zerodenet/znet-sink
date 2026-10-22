use super::*;
use crate::services::{app_config, common};
use serde_json::json;

fn raw(tag: &str) -> Value {
    json!({"tag_name":tag,"draft":false,"prerelease":false,"body":"real notes","assets":[]})
}

#[test]
fn android_candidates_require_canonical_origin_uploaded_size_and_api_digest() {
    let asset = json!({"id": 57, "name": "client-x86.apk", "size": 100,
        "state":"uploaded", "digest":format!("sha256:{}", "a".repeat(64)),
        "browser_download_url":"https://github.com/zerodenet/znet-sink/releases/download/v1.0.0/client-x86.apk"});
    let mut release = raw("v1.0.0");
    release["assets"] = json!([asset.clone()]);
    let result = parsed(
        vec![release.clone()],
        MobileReleaseSource::Client,
        MobileReleaseChannel::Stable,
    );
    assert_eq!(result[0].android_assets.len(), 1);
    assert_eq!(result[0].android_assets[0].asset_id, "57");
    for (field, invalid) in [
        ("digest", json!(null)),
        ("digest", json!(format!("sha256:{}", "A".repeat(64)))),
        ("size", json!(536870913u64)),
        ("state", json!("open")),
        ("id", json!(0)),
        ("name", json!("../client.apk")),
        (
            "browser_download_url",
            json!("https://evil.test/client-x86.apk"),
        ),
    ] {
        let mut bad = asset.clone();
        bad[field] = invalid;
        release["assets"] = json!([bad]);
        assert!(
            parsed(
                vec![release.clone()],
                MobileReleaseSource::Client,
                MobileReleaseChannel::Stable
            )[0]
            .android_assets
            .is_empty(),
            "{field}"
        );
    }
    release["assets"] = json!([asset]);
    assert!(parsed(
        vec![release],
        MobileReleaseSource::Kernel,
        MobileReleaseChannel::Stable
    )[0]
    .android_assets
    .is_empty());
}
fn parsed(
    values: Vec<Value>,
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
) -> Vec<MobileRelease> {
    parse_catalog(&serde_json::to_vec(&values).unwrap(), source, channel).unwrap()
}

#[test]
fn client_channels_match_existing_policy_without_desktop_asset_requirement() {
    let values = vec![
        raw("v1.0.0"),
        raw("v1.1.0-rc.1"),
        raw("v1.1.0-preview.2"),
        raw("v1.1.0-beta.2"),
        raw("v1.1.0-dev.4"),
    ];
    assert_eq!(
        parsed(
            values.clone(),
            MobileReleaseSource::Client,
            MobileReleaseChannel::Stable
        )
        .len(),
        1
    );
    assert_eq!(
        parsed(
            values.clone(),
            MobileReleaseSource::Client,
            MobileReleaseChannel::Preview
        )
        .len(),
        2
    );
    let test = parsed(
        values,
        MobileReleaseSource::Client,
        MobileReleaseChannel::Test,
    );
    assert_eq!(test.len(), 2);
    assert!(test.iter().all(|release| release.kernel_channel.is_none()));
}

#[test]
fn kernel_contract_preserves_beta_nightly_and_projects_into_mobile_labels() {
    let values = vec![
        raw("v1.0.0"),
        raw("v1.1.0-rc.1"),
        raw("v1.1.0-beta.2"),
        raw("v1.1.0-dev.3"),
    ];
    let candidates = parsed(
        values.clone(),
        MobileReleaseSource::Kernel,
        MobileReleaseChannel::Preview,
    );
    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|release| release.kernel_channel == Some(ReleaseChannel::Beta)));
    assert_eq!(
        parsed(
            values,
            MobileReleaseSource::Kernel,
            MobileReleaseChannel::Test
        )[0]
        .kernel_channel,
        Some(ReleaseChannel::Nightly)
    );
}

#[test]
fn draft_and_noncanonical_tags_are_not_published() {
    let mut draft = raw("v3.0.0");
    draft["draft"] = json!(true);
    let releases = parsed(
        vec![
            draft,
            raw("latest"),
            raw("v01.0.0"),
            raw("v1.0.0"),
            raw("v2.0.0"),
        ],
        MobileReleaseSource::Client,
        MobileReleaseChannel::Stable,
    );
    assert_eq!(
        releases
            .iter()
            .map(|v| v.version.as_str())
            .collect::<Vec<_>>(),
        vec!["2.0.0", "1.0.0"]
    );
}

#[test]
fn notes_are_bounded_utf8_plain_text_and_urls_are_project_owned() {
    let mut input = raw("v1.0.0");
    input["body"] = json!(format!(
        "<img src=x onerror=alert(1)>{}",
        "中".repeat(30_000)
    ));
    input["html_url"] = json!("https://secret.test/?token=private");
    let releases = parsed(
        vec![input],
        MobileReleaseSource::Client,
        MobileReleaseChannel::Stable,
    );
    assert!(releases[0].notes_truncated);
    assert!(releases[0].notes.as_ref().unwrap().len() <= MAX_NOTES_BYTES);
    assert!(releases[0].notes.as_ref().unwrap().starts_with("<img"));
    assert_eq!(
        releases[0].release_url,
        "https://github.com/zerodenet/znet-sink/releases/tag/v1.0.0"
    );
}

#[test]
fn invalid_or_excess_catalog_never_becomes_authoritative_empty_state() {
    for bytes in [
        b"null".to_vec(),
        b"{}".to_vec(),
        vec![b' '; MAX_BYTES + 1],
        serde_json::to_vec(&vec![raw("v1.0.0"); 31]).unwrap(),
        serde_json::to_vec(&vec![json!({"tag_name":"v9.0.0"})]).unwrap(),
        b"[false]".to_vec(),
    ] {
        assert!(parse_catalog(
            &bytes,
            MobileReleaseSource::Client,
            MobileReleaseChannel::Stable
        )
        .is_err());
    }
    assert!(parsed(
        vec![],
        MobileReleaseSource::Client,
        MobileReleaseChannel::Stable
    )
    .is_empty());
}

#[test]
fn preferences_only_candidate_preserves_network_and_ownership_and_checks_full_revision() {
    let mut config = AppConfig::default();
    config.core.auto_connect = true;
    config.profile_edits.insert(
        "profile-a".into(),
        BTreeMap::from([("endpoint.local".into(), json!(true))]),
    );
    let preferences = ReleasePreferences {
        client_auto_check: true,
        client_channel: MobileReleaseChannel::Test,
        kernel_channel: MobileReleaseChannel::Preview,
    };
    let revision = revision(&config).unwrap();
    let mut updated = candidate(&config, &revision, preferences.clone()).unwrap();
    assert_eq!(updated.release_preferences, preferences);
    updated.release_preferences = config.release_preferences.clone();
    assert_eq!(updated, config);
    config.ui.theme = "dark".into();
    assert_eq!(
        candidate(&config, &revision, preferences).unwrap_err().code,
        "conflict"
    );
}

#[test]
fn preferences_atomic_local_store_and_reload_retains_intent_without_runtime_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app-config.json");
    let current = AppConfig::default();
    let state = AppState::new(current.clone());
    let next = candidate(
        &current,
        &revision(&current).unwrap(),
        ReleasePreferences {
            client_auto_check: true,
            ..Default::default()
        },
    )
    .unwrap();
    app_config::compare_and_replace_local_at(&state, &current, next.clone(), &path).unwrap();
    assert_eq!(
        crate::services::app_config_store::load_or_default(&path)
            .unwrap()
            .release_preferences,
        next.release_preferences
    );
    assert_eq!(
        preferences(&next).unwrap().scheduler_state,
        if cfg!(any(target_os = "android", target_os = "ios")) {
            "native-foreground"
        } else {
            "not-native-host"
        }
    );
    assert_eq!(
        common::lock(state.app_config(), "app_config").unwrap().core,
        current.core
    );
}

#[test]
fn preferences_reject_unknown_permissions_or_malformed_channel_and_old_configs_default() {
    for input in [
        json!({"permissions":[]}),
        json!({"clientChannel":"rc"}),
        json!({"kernelChannel":"nightly"}),
        json!({"clientAutoCheck":"true"}),
    ] {
        assert!(serde_json::from_value::<ReleasePreferences>(input).is_err());
    }
    let old: AppConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(old.release_preferences, ReleasePreferences::default());
    let export = crate::services::settings_transfer::export(&AppConfig {
        release_preferences: ReleasePreferences {
            client_auto_check: true,
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    assert!(export.omitted_fields.contains(&"releasePreferences".into()));
    assert!(!export.document.contains("clientAutoCheck"));
}
