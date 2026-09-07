use super::parse_policy_groups;
use serde_json::json;

#[test]
fn empty_probe_results_preserve_selector_choices_and_confirmed_selection() {
    let groups = parse_policy_groups(&json!({ "policies": [{
        "tag": "Proxy", "kind": "selector", "selected": "Auto",
        "outbounds": ["Auto", "Japan"], "url_test_members": []
    }] }));
    assert_eq!(groups[0].selected.as_deref(), Some("Auto"));
    assert_eq!(groups[0].outbounds.len(), 2);
    assert_eq!(groups[0].outbounds[0].tag, "Auto");
    assert!(groups[0].outbounds[0].selected);
}

#[test]
fn probe_results_enrich_members_without_replacing_membership() {
    let groups = parse_policy_groups(&json!([{ "tag": "Auto", "kind": "urltest",
        "selected": "US", "outbounds": ["Japan", "US"],
        "url_test_members": [
            {"tag": "US", "alive": true, "latency_ms": 441, "last_checked_unix_ms": 1234},
            {"tag": "removed", "alive": true, "latency_ms": 1}
        ]
    }]));
    assert_eq!(groups[0].outbounds.len(), 2);
    assert_eq!(groups[0].outbounds[0].tag, "Japan");
    assert_eq!(groups[0].outbounds[0].alive, None);
    let selected = &groups[0].outbounds[1];
    assert!(selected.selected);
    assert_eq!(selected.delay_ms, Some(441));
    assert_eq!(selected.last_checked_unix_ms, Some(1234));
}

#[test]
fn legacy_probe_only_payloads_remain_readable_but_explicit_empty_membership_wins() {
    for (members, expected) in [(None, 1), (Some(json!([])), 0)] {
        let mut group = json!({ "tag": "Auto", "kind": "urltest", "selected": "US",
            "url_test_members": [{"tag": "US", "alive": true, "latency_ms": 441}]
        });
        if let Some(members) = members {
            group["outbounds"] = members;
        }
        assert_eq!(
            parse_policy_groups(&json!([group]))[0].outbounds.len(),
            expected
        );
    }
}
