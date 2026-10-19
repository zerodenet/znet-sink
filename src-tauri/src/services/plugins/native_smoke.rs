//! Fixed Debug-only VM acceptance. No guest input, network, files or user grants.
use super::*;
use std::{sync::atomic::AtomicBool, time::Duration};
use znet_plugin_sandbox::{
    contract::{sha256, Capability, Error},
    runtime::{Executor, ReviewedExecution},
};

fn exercise(executor: &Executor, target: Target) -> AppResult<serde_json::Value> {
    let permission = Request {
        capability: Capability::SelfRead,
        scope: "self".into(),
    };
    let mut checks = Vec::new();
    for (id, source, expected) in [
        ("arithmetic", "6*7", "arithmetic"),
        ("identity", "({answer:6*7,identity:JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))})", "value"),
        ("denied", "hostCall('{\"capability\":\"records.summary.read\",\"scope\":\"selection:smoke\"}')", "denied"),
        ("deadline", "while(true){}", "deadline"),
    ] {
        let limits = znet_plugin_sandbox::contract::Limits { timeout_ms:if expected == "deadline" {100} else {2000}, ..Default::default() };
        let manifest = serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.zerodenet.native-smoke","component_id":id,"version":"1.0.0","requires_host":format!("={}", env!("CARGO_PKG_VERSION")),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":[target.clone()],"required":[permission.clone()],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":limits});
        let component = Component::load(&serde_json::to_vec(&manifest).map_err(io::failure)?, source).map_err(io::failure)?;
        component.compatible(&target, env!("CARGO_PKG_VERSION"), Isolation::Vm).map_err(io::failure)?;
        // A private, disposable authority. The smoke never edits app consent.
        let manager = Manager::default();
        let authority = Authority::admit_with_manager(&component, &BTreeSet::from([permission.clone()]), &manager).map_err(io::failure)?;
        authority.authorize(BTreeSet::from([permission.clone()]), Duration::from_secs(10)).map_err(io::failure)?;
        let review = manager.component_snapshots().into_iter().find(|row| !row.retired).ok_or_else(stale)?;
        let run = || executor.execute_reviewed(&component, &authority, serde_json::Value::Null, ReviewedExecution {
            registration:review.registration, revision:review.revision,
            cancelled:Arc::new(AtomicBool::new(false)), host_version:env!("CARGO_PKG_VERSION").into(), dispatcher:None, scheduled:false,
        }).and_then(|receipt| receipt.into_value());
        let result = run();
        let passed = match expected {
            "arithmetic" => result.as_ref().is_ok_and(|value| value.as_i64() == Some(42)),
            "value" => result.as_ref().is_ok_and(|value| value["answer"] == 42 && value["identity"]["plugin_id"] == "org.zerodenet.native-smoke"),
            "denied" => matches!(result, Err(Error::PermissionDenied)),
            _ => matches!(result, Err(Error::Deadline)),
        };
        if !passed {return Err(AppError {code:"native_vm_smoke_failed",message:"固定原生 VM 验证未通过".into(),details:Some(serde_json::json!({"fixture":id,"actual":result.as_ref().err().map(|error|format!("{error:?}")).unwrap_or_else(||"unexpected-value".into())}))});}
        authority.revoke();
        if run().is_ok() {return Err(AppError::internal("固定 VM 撤权验证未通过"));}
        checks.push(expected);
    }
    Ok(
        serde_json::json!({"executed":true,"arithmetic":checks.contains(&"arithmetic"),"identity":checks.contains(&"value"),"ungrantedRejected":checks.contains(&"denied"),"deadlineEnforced":checks.contains(&"deadline"),"revocationEnforced":true,"target":target,"isolation":"bounded-in-process-interpreter"}),
    )
}
impl Host {
    pub fn native_vm_smoke(&self) -> AppResult<serde_json::Value> {
        let nonce = std::env::var("ZNET_IOS_UI_SMOKE_NONCE").unwrap_or_default();
        if !cfg!(target_os = "ios")
            || nonce.len() != 32
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(platform::unavailable(
                "仅显式启动的 Debug 原生验收可执行固定 VM 检查",
            ));
        }
        let (executor, target, generation) = {
            let _operation = self.operation_guard()?;
            (
                self.dispatch_executor()?,
                self.platform.target()?,
                self.platform.review_generation(),
            )
        };
        let result = exercise(&executor, target)?;
        self.platform
            .check_review_generation(generation.as_deref())?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_native_acceptance_uses_real_vm_and_permissions() {
        let value = exercise(&Executor::default(), Target::native_desktop().unwrap()).unwrap();
        assert_eq!(value["executed"], true);
        assert_eq!(value["revocationEnforced"], true);
    }
    #[test]
    fn ordinary_debug_launch_cannot_request_native_acceptance() {
        assert!(Host::default().native_vm_smoke().is_err());
    }
}
