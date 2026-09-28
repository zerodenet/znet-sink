use super::*;

fn profile(action: CommonRuleAction) -> RuleSetProfile {
    serde_json::from_value(json!({"id":"local","name":"Local","enabled":true,
        "semanticIr":{"rules":[]},"updatedAtUnixMs":1,
        "commonBinding":{"enabled":true,"action":action,"order":0},
        "artifact":{"path":"test.zrs","majorVersion":1,"minorVersion":0,"checksum":0,
        "fileSize":1,"entryCount":1,"builtAtUnixMs":1}}))
    .unwrap()
}

#[test]
fn following_final_route_inherits_its_packet_or_flow_path_without_changing_source() {
    for mode in [None, Some("auto"), Some("flow"), Some("packet")] {
        let mut base = json!({"schema_version":1,"mode":{"type":"rule"},"route":{
            "rules":[{"condition":{"type":"domain","values":["private.test"]},
                      "action":{"type":"route","outbound":"wg"},"mode":"flow"}],
            "auto_outbounds":["wg"],"final":{"type":"route","outbound":"wg"}},
            "outbounds":[{"tag":"wg","protocol":{"type":"wireguard","private_key":"secret"}}]});
        if let Some(mode) = mode {
            base["route"]["final_mode"] = json!(mode);
        }
        let original = base.clone();
        let result = compose_with(&base, true, &[profile(CommonRuleAction::Final)], |_, _| {
            Ok(())
        })
        .unwrap()
        .config;
        assert_eq!(base, original);
        assert_eq!(result["outbounds"], base["outbounds"]);
        assert_eq!(result["route"]["rules"][0], base["route"]["rules"][0]);
        assert_eq!(
            result["route"]["auto_outbounds"],
            base["route"]["auto_outbounds"]
        );
        assert_eq!(
            result["route"]["rules"][1].get("mode"),
            mode.map(|m| json!(m)).as_ref()
        );
        let repeated = compose_with(
            &result,
            true,
            &[profile(CommonRuleAction::Final)],
            |_, _| Ok(()),
        )
        .unwrap()
        .config;
        assert_eq!(result, repeated);
    }
}

#[test]
fn explicit_action_does_not_inherit_an_unrelated_final_packet_mode() {
    let base = json!({"mode":{"type":"rule"},"route":{
        "final":{"type":"route","outbound":"wg"},"final_mode":"packet"}});
    for action in [
        CommonRuleAction::Direct,
        CommonRuleAction::Reject,
        CommonRuleAction::Proxy,
    ] {
        let result = compose_with(&base, true, &[profile(action)], |_, _| Ok(()))
            .unwrap()
            .config;
        assert!(result["route"]["rules"][0].get("mode").is_none());
    }
}
