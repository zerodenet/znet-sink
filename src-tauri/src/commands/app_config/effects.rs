use crate::models::app_config::AppConfig;

pub(super) struct Effects {
    pub restart: bool,
    pub recompose: bool,
    pub retarget_proxy: bool,
}

pub(super) fn between(old: &AppConfig, next: &AppConfig) -> Effects {
    let endpoint = old.local_proxy.host != next.local_proxy.host
        || old.local_proxy.port != next.local_proxy.port;
    Effects {
        restart: old.core.executable_path != next.core.executable_path
            || old.core.socket != next.core.socket
            || old.core.working_dir != next.core.working_dir
            || old.core.config_path != next.core.config_path,
        recompose: endpoint || old.url_test != next.url_test || old.routing != next.routing,
        retarget_proxy: endpoint || old.local_proxy.bypass != next.local_proxy.bypass,
    }
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
