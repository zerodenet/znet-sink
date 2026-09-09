use crate::models::app_config::AppConfig;

pub(super) struct Effects {
    pub restart: bool,
    pub recompose: bool,
    pub retarget_proxy: bool,
}

pub(super) fn between(old: &AppConfig, next: &AppConfig) -> Effects {
    let endpoint = old.local_proxy.host != next.local_proxy.host
        || old.local_proxy.port != next.local_proxy.port;
    let precedence = old.overrides != next.overrides;
    Effects {
        restart: precedence
            || old.core.executable_path != next.core.executable_path
            || old.core.socket != next.core.socket
            || old.core.working_dir != next.core.working_dir
            || old.core.config_path != next.core.config_path
            || (old.bypass != next.bypass
                && crate::services::bypass::network_rules_changed(old, next)),
        recompose: endpoint
            || old.url_test != next.url_test
            || old.routing != next.routing
            || old.bypass != next.bypass,
        retarget_proxy: precedence || endpoint || old.local_proxy.bypass != next.local_proxy.bypass,
    }
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
