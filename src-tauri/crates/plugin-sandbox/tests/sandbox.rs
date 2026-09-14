use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_plugin_sandbox::{
    contract::*,
    policy::Authority,
    runtime::{execute, Summary},
};
fn request() -> Request {
    Request {
        capability: Capability::RecordsSummaryRead,
        scope: "selection:demo".into(),
    }
}
fn manifest(source: &str) -> Manifest {
    Manifest {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.test".into(),
        component_id: "summary".into(),
        version: "1.0.0".into(),
        requires_host: "=0.0.1".into(),
        api_version: 1,
        runtime: "javascript-v1".into(),
        minimum_isolation: Isolation::Vm,
        targets: Targets::Only(vec![Target::native_desktop().unwrap()]),
        required: vec![request()],
        optional: vec![],
        source_sha256: sha256(source.as_bytes()),
        limits: Limits {
            timeout_ms: 100,
            ..Limits::default()
        },
    }
}
fn component(source: &str) -> Component {
    Component::load(&serde_json::to_vec(&manifest(source)).unwrap(), source).unwrap()
}
fn authority(component: &Component) -> Authority {
    let grants = BTreeSet::from([request()]);
    let auth = Authority::admit(component, &grants).unwrap();
    auth.authorize(grants, Duration::from_secs(60)).unwrap();
    auth
}
fn run(source: &str) -> Result<serde_json::Value, Error> {
    let c = component(source);
    execute(
        &c,
        &authority(&c),
        Some((
            "demo".into(),
            Summary {
                records: 3,
                upload_bytes: 4,
                download_bytes: 5,
            },
        )),
        Arc::new(AtomicBool::new(false)),
    )
}
#[test]
fn selected_summary_round_trip() {
    assert_eq!(run(r#"JSON.parse(hostCall(JSON.stringify({capability:'records.summary.read',scope:'selection:demo'})))"#).unwrap()["records"], 3);
}
#[test]
fn default_denied_required_and_admission_ceiling() {
    let c = component("1");
    assert!(matches!(
        Authority::admit(&c, &BTreeSet::new()),
        Err(Error::AdmissionDenied)
    ));
    let auth = Authority::admit(&c, &BTreeSet::from([request()])).unwrap();
    assert!(matches!(auth.begin(&c), Err(Error::Disabled)));
    auth.authorize(BTreeSet::new(), Duration::from_secs(1))
        .unwrap();
    assert!(matches!(auth.begin(&c), Err(Error::PermissionDenied)));
}
#[test]
fn permissions_cannot_be_bypassed_or_errors_swallowed() {
    assert_eq!(
        run(
            r#"try { hostCall('{"capability":"records.summary.read","scope":"selection:other"}'); } catch(e) {} 'leak'"#
        ),
        Err(Error::PermissionDenied)
    );
    assert_eq!(
        run(r#"hostCall('{"capability":"filesystem.read","scope":"all"}')"#),
        Err(Error::PermissionDenied)
    );
}
#[test]
fn host_selection_and_grant_must_both_match() {
    let c =
        component(r#"hostCall('{"capability":"records.summary.read","scope":"selection:demo"}')"#);
    assert_eq!(
        execute(
            &c,
            &authority(&c),
            Some((
                "other".into(),
                Summary {
                    records: 1,
                    upload_bytes: 0,
                    download_bytes: 0
                }
            )),
            Arc::new(AtomicBool::new(false))
        ),
        Err(Error::PermissionDenied)
    );
}
#[test]
fn no_ambient_host_apis_or_dynamic_function_constructor() {
    let output = run(r#"[typeof fetch,typeof process,typeof require,typeof window,typeof __TAURI__,typeof eval,typeof Function,typeof (()=>{}).constructor,typeof (function*(){}).constructor,typeof (async()=>{}).constructor,typeof (async function*(){}).constructor]"#).unwrap();
    assert!(output.as_array().unwrap().iter().all(|v| v == "undefined"));
    assert!(run("import('std')").is_err());
}
#[test]
fn infinite_loop_and_serialization_loop_are_interrupted() {
    assert_eq!(run("while(true){}"), Err(Error::Deadline));
    assert_eq!(run("({toJSON(){while(true){}}})"), Err(Error::Deadline));
}
#[test]
fn output_memory_stack_and_call_budgets() {
    assert_eq!(run("'x'.repeat(70000)"), Err(Error::BudgetExceeded));
    assert!(run("'x'.repeat(100000000)").is_err());
    assert!(run("function recurse(){ return 1+recurse(); } recurse()").is_err());
    assert_eq!(
        run(
            r#"for(let i=0;i<33;i++) hostCall('{"capability":"records.summary.read","scope":"selection:demo"}'); 1"#
        ),
        Err(Error::BudgetExceeded)
    );
    assert_eq!(
        run("hostCall('x'.repeat(131073))"),
        Err(Error::BudgetExceeded)
    );
}
#[test]
fn leases_bind_digest_and_serialize_execution() {
    let c = component("1");
    let a = authority(&c);
    let lease = a.begin(&c).unwrap();
    assert!(matches!(a.begin(&c), Err(Error::Busy)));
    assert!(matches!(
        a.begin(&component("2")),
        Err(Error::DigestMismatch)
    ));
    a.revoke();
    assert_eq!(lease.check(None), Err(Error::Revoked));
    drop(lease);
    a.authorize(BTreeSet::from([request()]), Duration::from_secs(1))
        .unwrap();
    assert!(a.begin(&c).is_ok());
}
#[test]
fn revoked_running_guest_and_pre_cancelled_guest_release_no_result() {
    let c = component("while(true){}");
    let auth = authority(&c);
    let revoked = auth.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        revoked.revoke();
    });
    assert_eq!(
        execute(&c, &auth, None, Arc::new(AtomicBool::new(false))),
        Err(Error::Revoked)
    );
    thread.join().unwrap();
    let c = component("42");
    assert_eq!(
        execute(&c, &authority(&c), None, Arc::new(AtomicBool::new(true))),
        Err(Error::Cancelled)
    );
}
#[test]
fn expiration_and_reauthorization_invalidate_previous_lease() {
    let c = component("1");
    let auth = authority(&c);
    auth.authorize(BTreeSet::from([request()]), Duration::from_millis(1))
        .unwrap();
    let lease = auth.begin(&c).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(lease.check(None), Err(Error::Expired));
    auth.authorize(BTreeSet::from([request()]), Duration::from_secs(1))
        .unwrap();
    assert_eq!(lease.check(None), Err(Error::Revoked));
}
#[test]
fn manifest_and_device_admission_fail_closed() {
    let mut m = manifest("1");
    m.minimum_isolation = Isolation::Process;
    let c = Component::load(&serde_json::to_vec(&m).unwrap(), "1").unwrap();
    assert_eq!(
        execute(&c, &authority(&c), None, Arc::new(AtomicBool::new(false))),
        Err(Error::UnsupportedIsolation)
    );
    let mobile = Target {
        os: Os::Android,
        arch: Arch::Aarch64,
        device: DeviceClass::Phone,
    };
    assert_eq!(
        c.compatible(&mobile, "0.0.1", Isolation::Process),
        Err(Error::IncompatibleDevice)
    );
    assert_eq!(
        c.compatible(
            &Target::native_desktop().unwrap(),
            "9.0.0",
            Isolation::Process
        ),
        Err(Error::IncompatibleVersion)
    );
    assert!(matches!(
        Component::load(&serde_json::to_vec(&m).unwrap(), "2"),
        Err(Error::DigestMismatch)
    ));
    m.host = "zboard".into();
    assert!(matches!(
        Component::load(&serde_json::to_vec(&m).unwrap(), "1"),
        Err(Error::WrongHost)
    ));
}
#[test]
fn fresh_runtime_has_no_previous_guest_globals() {
    assert_eq!(run("globalThis.secret=123; 1").unwrap(), 1);
    assert_eq!(run("typeof secret").unwrap(), "undefined");
}

#[test]
fn duplicate_component_admission_cannot_bypass_a_running_vm_lease() {
    let manager = znet_client_core::capability::Manager::default();
    let c = component("1");
    let ceiling = BTreeSet::from([request()]);
    let first = Authority::admit_with_manager(&c, &ceiling, &manager).unwrap();
    first
        .authorize(ceiling.clone(), Duration::from_secs(60))
        .unwrap();
    let lease = first.begin(&c).unwrap();
    let duplicate = Authority::admit_with_manager(&c, &ceiling, &manager).unwrap();
    assert!(matches!(duplicate.begin(&c), Err(Error::Busy)));
    manager.remove_component("org.example.test/summary");
    assert_eq!(lease.check(None), Err(Error::Revoked));
    assert_eq!(
        duplicate.authorize(ceiling, Duration::from_secs(60)),
        Err(Error::Revoked)
    );
}

#[test]
fn host_revocation_interrupts_a_real_vm_and_releases_its_running_slot() {
    let manager = znet_client_core::capability::Manager::default();
    let source = "while (true) {}";
    let mut manifest = manifest(source);
    manifest.limits.timeout_ms = 2000;
    let c = Component::load(&serde_json::to_vec(&manifest).unwrap(), source).unwrap();
    let ceiling = BTreeSet::from([request()]);
    let authority = Authority::admit_with_manager(&c, &ceiling, &manager).unwrap();
    authority
        .authorize(ceiling, Duration::from_secs(60))
        .unwrap();
    let worker =
        std::thread::spawn(move || execute(&c, &authority, None, Arc::new(AtomicBool::new(false))));
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !manager.component_snapshots()[0].running {
        assert!(std::time::Instant::now() < deadline, "VM failed to start");
        std::thread::sleep(Duration::from_millis(1));
    }
    manager.revoke_component("org.example.test/summary");
    assert_eq!(worker.join().unwrap(), Err(Error::Revoked));
    let snapshot = &manager.component_snapshots()[0];
    assert!(!snapshot.running);
    assert!(!snapshot.enabled);
}

#[test]
fn guest_exception_reclaims_vm_state_without_changing_user_grants() {
    let manager = znet_client_core::capability::Manager::default();
    let c = component("throw new Error('guest')");
    let ceiling = BTreeSet::from([request()]);
    let authority = Authority::admit_with_manager(&c, &ceiling, &manager).unwrap();
    authority
        .authorize(ceiling.clone(), Duration::from_secs(60))
        .unwrap();
    for _ in 0..2 {
        let same = Authority::admit_with_manager(&c, &ceiling, &manager).unwrap();
        assert_eq!(
            execute(&c, &same, None, Arc::new(AtomicBool::new(false))),
            Err(Error::GuestException)
        );
        let snapshot = &manager.component_snapshots()[0];
        assert!(!snapshot.running);
        assert!(snapshot.enabled);
    }
}
