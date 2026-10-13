use znet_client_core::mobile::*;

fn identity() -> RuntimeIdentity {
    RuntimeIdentity {
        core_instance_id: "actual-core-instance".into(),
        profile_id: "profile-1".into(),
        config_revision: "18446744073709551615".into(),
    }
}

fn evidence() -> (RuntimeEvidence, PlatformEvidence) {
    (
        RuntimeEvidence {
            identity: identity(),
            proxy_running: true,
            external_tun_attached: true,
            protected_egress: true,
            dns_confirmed: true,
        },
        PlatformEvidence {
            identity: identity(),
            authorized: true,
            tunnel_established: true,
            physical_network_available: true,
        },
    )
}

#[test]
fn missing_platform_evidence_does_not_confirm_disconnection() {
    let result = reconcile(None, None);
    assert_eq!(result.vpn_state, VpnState::Unknown);
    assert_eq!(result.kernel_state, KernelState::Unavailable);
    assert_eq!(result.identity, None);
}

#[test]
fn proxy_and_platform_must_jointly_confirm_takeover() {
    let (runtime, platform) = evidence();
    assert_eq!(
        reconcile(Some(&runtime), Some(&platform)).vpn_state,
        VpnState::TakenOver
    );
    for missing in [0, 1, 2, 3, 4] {
        let (mut runtime, mut platform) = evidence();
        match missing {
            0 => runtime.proxy_running = false,
            1 => runtime.external_tun_attached = false,
            2 => runtime.protected_egress = false,
            3 => runtime.dns_confirmed = false,
            _ => platform.physical_network_available = false,
        }
        assert_eq!(
            reconcile(Some(&runtime), Some(&platform)).vpn_state,
            VpnState::Recovering
        );
    }
}

#[test]
fn revocation_is_immediate_and_independent_of_kernel_readiness() {
    let (runtime, mut platform) = evidence();
    platform.authorized = false;
    let result = reconcile(Some(&runtime), Some(&platform));
    assert_eq!(result.kernel_state, KernelState::Ready);
    assert_eq!(result.vpn_state, VpnState::NotAuthorized);
}

#[test]
fn platform_off_leaves_a_ready_kernel() {
    let (runtime, mut platform) = evidence();
    platform.tunnel_established = false;
    let result = reconcile(Some(&runtime), Some(&platform));
    assert_eq!(result.kernel_state, KernelState::Ready);
    assert_eq!(result.vpn_state, VpnState::Off);
}

#[test]
fn every_identity_dimension_invalidates_old_evidence() {
    let (runtime, platform) = evidence();
    for dimension in [0, 1, 2] {
        let mut changed = platform.clone();
        match dimension {
            0 => changed.identity.core_instance_id = "new-instance".into(),
            1 => changed.identity.profile_id = "profile-2".into(),
            _ => changed.identity.config_revision = "new-revision".into(),
        }
        let result = reconcile(Some(&runtime), Some(&changed));
        assert_eq!(result.vpn_state, VpnState::Unknown);
        assert_eq!(result.identity, None);
    }
}

#[test]
fn blank_identities_cannot_confirm_takeover() {
    let (mut runtime, mut platform) = evidence();
    runtime.identity.core_instance_id.clear();
    platform.identity.core_instance_id.clear();
    assert_eq!(
        reconcile(Some(&runtime), Some(&platform)).vpn_state,
        VpnState::Unknown
    );
}

#[test]
fn stopping_or_revoking_prevents_a_queued_recovery_from_restarting_vpn() {
    let mut intent = RecoveryIntent::default();
    assert!(intent.ticket(identity()).is_none());
    intent.set_enabled(true);
    let ticket = intent.ticket(identity()).unwrap();
    assert!(intent.accepts(&ticket, &identity()));
    intent.set_enabled(false);
    assert!(!intent.accepts(&ticket, &identity()));
    intent.set_enabled(true);
    assert!(!intent.accepts(&ticket, &identity()));
}

#[test]
fn recovery_never_controls_a_replaced_instance_or_profile() {
    let mut intent = RecoveryIntent::default();
    intent.set_enabled(true);
    let ticket = intent.ticket(identity()).unwrap();
    let mut current = identity();
    current.profile_id = "new-profile".into();
    assert!(!intent.accepts(&ticket, &current));
}

#[test]
fn desktop_commands_are_gated_while_local_drafts_are_available() {
    for command in [
        "core_process_start",
        "core_ipc_request",
        "system_proxy_enable",
        "gui_tun_enable",
        "gui_disconnect",
        "kernel_install_version",
        "platform_open_path",
        "tray_update_status",
    ] {
        assert!(requires_desktop_host(command), "{command}");
    }
    for command in [
        "proxy_config_list",
        "proxy_config_upsert",
        "subscription_list",
        "gui_mobile_platform_snapshot",
        "platform_open_url",
        "platform_clipboard_write",
    ] {
        assert!(!requires_desktop_host(command), "{command}");
    }
}

#[test]
fn unadapted_runtime_reads_and_writes_cannot_fall_back_to_desktop_ipc() {
    for command in [
        "gui_select_policy",
        "gui_set_proxy_mode",
        "gui_set_mode",
        "gui_probe_job_start",
        "gui_tool_job_start",
        "gui_policy_groups",
        "gui_core_overview",
        "gui_observation_snapshot",
        "gui_endpoint_catalog",
        "gui_packet_routes_snapshot",
        "gui_close_packet_route",
        "gui_traffic_reset",
        "gui_validate_dns_config",
        "gui_config_workspace_apply",
        "core_get_runtime",
    ] {
        assert!(
            requires_desktop_host(command),
            "runtime fallback was allowed: {command}"
        );
    }
    for command in [
        "gui_client_core_snapshot",
        "gui_node_screen_snapshot",
        "gui_proxy_nodes",
        "gui_config_policy_groups",
        "gui_config_workspace_snapshot",
        "gui_inspect_dns_effective_config",
        "gui_apply_dns_config",
        "gui_probe_job_list",
        "gui_tool_job_list",
        "proxy_config_import",
        "proxy_config_set_active",
        "profile_settings_apply",
    ] {
        assert!(
            !requires_desktop_host(command),
            "offline operation was blocked: {command}"
        );
    }
}

#[test]
fn offline_mobile_publication_skips_the_unavailable_desktop_effect() {
    let mut attempted = false;
    let result: Result<Option<()>, &str> = desktop_profile_effect(true, || {
        attempted = true;
        Err("desktop proxy unavailable")
    });
    assert_eq!(result, Ok(None));
    assert!(!attempted);
    let desktop: Result<Option<()>, &str> = desktop_profile_effect(false, || {
        attempted = true;
        Err("native desktop proxy read failed")
    });
    assert_eq!(desktop, Err("native desktop proxy read failed"));
    assert!(attempted);
}
