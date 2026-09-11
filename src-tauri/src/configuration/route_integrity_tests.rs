use super::*;
use serde_json::json;

#[test]
fn missing_nested_and_reject_rule_dependencies_cannot_be_dropped() {
    for condition in [
        json!({"type":"rule_set","tag":"deny"}),
        json!({"type":"and","conditions":[{"type":"not","condition":{"type":"rule_set","tag":"deny"}}]}),
    ] {
        let config = json!({"route":{"rules":[{"condition":condition,"action":{"type":"reject"}}],"final":{"type":"direct"}}});
        let before = config.clone();
        let error = validate(&config).unwrap_err();
        assert_eq!(error.code, "rule_set_dependency_unavailable");
        assert_eq!(error.details.unwrap()["missingRuleSets"], json!(["deny"]));
        assert_eq!(config, before);
    }
}

#[test]
fn complete_and_dependency_free_policies_remain_usable() {
    validate(&json!({"route":{"rule_sets":[{"tag":"deny"}],"rules":[{"condition":{"type":"rule_set","tag":"deny"},"action":{"type":"reject"}}]}})).unwrap();
    validate(&json!({"route":{"rules":[],"final":{"type":"direct"}}})).unwrap();
}
