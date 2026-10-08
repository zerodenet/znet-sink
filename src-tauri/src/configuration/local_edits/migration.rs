use super::{AppConfig, Edits};
use serde_json::{json, Value};

/// Promote the active profile's old network settings once, without choosing
/// conflicting values from inactive profiles. Keep old records for recovery;
/// resolve() only reads their endpoint preferences after this migration.
pub(crate) fn migrate_legacy(app: &mut AppConfig, id: Option<&str>) -> bool {
    let mut migrated = false;
    if app.client_edits.is_none() {
        let edits = id
            .and_then(|id| app.profile_edits.get(id))
            .cloned()
            .unwrap_or_default();
        app.client_edits = Some(
            edits
                .into_iter()
                .filter(|(key, _)| key != "endpoints")
                .collect(),
        );
        migrated = true;
    }
    if app.overrides == Default::default() {
        return migrated;
    }
    let flags = app.overrides.clone();
    let mut values = Edits::new();
    if flags.listener {
        values.insert("localProxy.host".into(), json!(app.local_proxy.host));
        values.insert("localProxy.port".into(), json!(app.local_proxy.port));
    }
    if flags.url_test {
        values.insert("urlTest.url".into(), json!(app.url_test.url));
        values.insert(
            "urlTest.toleranceMs".into(),
            json!(app.url_test.tolerance_ms),
        );
    }
    if flags.dns {
        values.insert("dns".into(), json!(app.dns));
    }
    if flags.bypass {
        if let Some(bypass) = &app.bypass {
            values.insert("bypass".into(), json!(bypass));
        }
    }
    if flags.rules {
        values.insert(
            "routing.injectCommonRules".into(),
            json!(app.routing.inject_common_rules),
        );
    }
    if flags.tun {
        if let Value::Object(tun) = json!(app.tun) {
            for (field, value) in tun {
                if matches!(
                    field.as_str(),
                    "name"
                        | "tag"
                        | "addr"
                        | "secondaryAddr"
                        | "mtu"
                        | "includeCidrs"
                        | "excludeCidrs"
                        | "dualStack"
                        | "dnsHijack"
                        | "autoRoute"
                        | "strictRoute"
                ) {
                    values.insert(format!("tun.{field}"), value);
                }
            }
        }
    }
    let edits = app.client_edits.as_mut().unwrap();
    for (key, value) in values {
        edits.entry(key).or_insert(value);
    }
    app.overrides = Default::default();
    true
}
