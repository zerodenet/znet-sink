use gui_lib::models::app_config::AppConfig;
use gui_lib::services::app_config::normalize_menu_keys;

#[test]
fn default_app_config_is_gui_schema() {
    let config = AppConfig::default();

    assert_eq!(config.schema_version, "gui.app.v1");
    assert_eq!(config.core.kernel, "zero");
    assert!(config.core.auto_connect);
    assert!(!config.core.auto_start);
    assert_eq!(config.logs.level, "info");
    assert!(!config.ui.sidebar_collapsed);
    assert!(config.ui.hidden_menu_keys.is_empty());
    assert!(config.ui.default_route.is_none());
    assert_eq!(config.local_proxy.host, "127.0.0.1");
    assert_eq!(config.local_proxy.port, 7890);
    assert!(config.local_proxy.source_proxy_config_id.is_none());
}

#[test]
fn hidden_menu_keys_are_normalized() {
    let keys = normalize_menu_keys(vec![
        " core ".to_string(),
        "".to_string(),
        "logs".to_string(),
        "core".to_string(),
    ]);

    assert_eq!(keys, vec!["core".to_string(), "logs".to_string()]);
}
