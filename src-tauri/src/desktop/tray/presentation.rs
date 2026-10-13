#[derive(Debug, PartialEq, Eq)]
pub(super) struct TrayPresentation {
    pub(super) proxy_recovery_required: bool,
    pub(super) system_proxy_label: &'static str,
    pub(super) status_label: &'static str,
    pub(super) action_label: &'static str,
    pub(super) capture_enabled: bool,
    pub(super) individual_controls_enabled: bool,
}

pub(super) fn tray_presentation(
    ui_mode: &str,
    running: bool,
    system_proxy_enabled: bool,
    tun_enabled: bool,
    system_proxy_owned: bool,
    proxy_status_known: bool,
) -> TrayPresentation {
    let lite_mode = ui_mode.eq_ignore_ascii_case("lite");
    let capture_enabled = if lite_mode {
        system_proxy_enabled || tun_enabled
    } else {
        system_proxy_enabled
    };
    let unmanaged = system_proxy_enabled && !system_proxy_owned;
    let proxy_recovery_required = unmanaged || !proxy_status_known;
    let status_label = if !proxy_status_known {
        "系统代理待确认"
    } else if unmanaged && tun_enabled {
        "TUN · 系统代理未接管"
    } else if unmanaged {
        "系统代理未接管"
    } else if system_proxy_enabled && tun_enabled {
        "系统代理 + TUN"
    } else if tun_enabled {
        "TUN 运行中"
    } else if system_proxy_enabled {
        "系统代理运行中"
    } else if running {
        "待机"
    } else {
        "已停止"
    };

    TrayPresentation {
        proxy_recovery_required,
        system_proxy_label: if !proxy_status_known {
            "系统代理（待确认）"
        } else if unmanaged {
            "系统代理（未接管）"
        } else {
            "系统代理"
        },
        status_label,
        action_label: if proxy_recovery_required {
            "处理系统代理…"
        } else if capture_enabled {
            "关闭代理"
        } else {
            "开启代理"
        },
        capture_enabled,
        individual_controls_enabled: running && !lite_mode && proxy_status_known,
    }
}
