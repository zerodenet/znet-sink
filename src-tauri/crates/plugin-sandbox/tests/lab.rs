use std::{path::PathBuf, process::Command};
#[test]
fn executable_runs_sample_and_refuses_missing_grants() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("examples/plugins/record-summary");
    let run = |grants: &str| {
        Command::new(env!("CARGO_BIN_EXE_znet-plugin-lab"))
            .arg(root.join("manifest.json"))
            .arg(root.join("main.js"))
            .arg(root.join(grants))
            .arg(root.join("summary.json"))
            .output()
            .unwrap()
    };
    let allowed = run("grants.json");
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&allowed.stdout).unwrap();
    assert_eq!(value, serde_json::json!({"records":3,"total_bytes":1000}));
    let denied = run("denied.json");
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("PermissionDenied"));
}
