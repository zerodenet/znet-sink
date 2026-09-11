use crate::models::app_config::AppConfig;

pub(super) struct Effects {
    pub restart: bool,
    pub recompose: bool,
    pub retarget_proxy: bool,
}

pub(super) fn between(old: &AppConfig, next: &AppConfig) -> Effects {
    let endpoint = old.local_proxy.host != next.local_proxy.host
        || old.local_proxy.port != next.local_proxy.port;
    let local = old.profile_edits != next.profile_edits;
    let changed = |prefix: &str| {
        old.profile_edits
            .iter()
            .chain(next.profile_edits.iter())
            .any(|(id, edits)| {
                edits
                    .keys()
                    .filter(|key| key.starts_with(prefix))
                    .any(|key| {
                        old.profile_edits.get(id).and_then(|v| v.get(key))
                            != next.profile_edits.get(id).and_then(|v| v.get(key))
                    })
            })
    };
    let capture_changed = changed("tun.") || changed("dns") || changed("bypass");
    let proxy_changed = changed("localProxy.") || changed("bypass");
    let precedence = old.overrides != next.overrides;
    Effects {
        restart: capture_changed
            || precedence
            || old.core.executable_path != next.core.executable_path
            || old.core.socket != next.core.socket
            || old.core.working_dir != next.core.working_dir
            || old.core.config_path != next.core.config_path
            || (old.bypass != next.bypass
                && crate::services::bypass::network_rules_changed(old, next)),
        recompose: local
            || endpoint
            || old.url_test != next.url_test
            || old.routing != next.routing
            || old.bypass != next.bypass,
        retarget_proxy: proxy_changed
            || precedence
            || endpoint
            || old.local_proxy.bypass != next.local_proxy.bypass,
    }
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
