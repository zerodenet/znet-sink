use super::environment::{proxy_environment_command, proxy_environment_no_proxy};
use super::presentation::tray_presentation;
use crate::models::app_config::default_proxy_bypass;

#[test]
fn copied_proxy_environment_uses_http_and_socks_endpoints() {
    let command = proxy_environment_command("127.0.0.1", 7890, &default_proxy_bypass());

    assert!(command.contains("HTTP_PROXY"));
    assert!(command.contains("http://127.0.0.1:7890"));
    assert!(command.contains("ALL_PROXY"));
    assert!(command.contains("socks5h://127.0.0.1:7890"));
    assert!(command.contains("NO_PROXY"));
    assert!(command.contains("192.168.0.0/16"));
    assert!(command.contains("10.0.0.0/8"));
}

#[test]
fn terminal_no_proxy_converts_windows_lan_wildcards_to_cidr() {
    let bypass = vec![
        "<local>".to_string(),
        "127.*".to_string(),
        "[::1]".to_string(),
        "172.16.*".to_string(),
        "192.168.*".to_string(),
        "intranet.example".to_string(),
    ];

    assert_eq!(
        proxy_environment_no_proxy(&bypass),
        "localhost,127.0.0.0/8,::1,172.16.0.0/16,192.168.0.0/16,intranet.example"
    );
}

#[test]
fn lite_tray_treats_partial_capture_as_an_active_session() {
    let presentation = tray_presentation("lite", true, false, true);

    assert_eq!(presentation.status_label, "TUN 运行中");
    assert_eq!(presentation.action_label, "关闭代理");
    assert!(!presentation.individual_controls_enabled);
}

#[test]
fn pro_tray_keeps_tun_and_system_proxy_independent() {
    let presentation = tray_presentation("pro", true, false, true);

    assert_eq!(presentation.status_label, "TUN 运行中");
    assert_eq!(presentation.action_label, "开启代理");
    assert!(presentation.individual_controls_enabled);
}

#[test]
fn tray_reports_combined_capture_and_stopped_states() {
    let combined = tray_presentation("pro", true, true, true);
    let stopped = tray_presentation("lite", false, false, false);

    assert_eq!(combined.status_label, "系统代理 + TUN");
    assert_eq!(combined.action_label, "关闭代理");
    assert_eq!(stopped.status_label, "已停止");
    assert_eq!(stopped.action_label, "开启代理");
}
