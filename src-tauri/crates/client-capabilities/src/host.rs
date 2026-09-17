//! Policies created by trusted native client code. Guest components never
//! receive these constructors or choose their own budgets.
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Error, Lease, Manager, Permission};

pub fn network(
    manager: &Manager,
    identity: &str,
    capability: &str,
    calls: u32,
    bytes: usize,
    timeout: Duration,
) -> Result<Lease, Error> {
    operation(
        manager,
        identity,
        Permission::new(capability, "*"),
        calls,
        bytes,
        timeout,
    )
}

pub fn operation(
    manager: &Manager,
    identity: &str,
    permission: Permission,
    calls: u32,
    bytes: usize,
    timeout: Duration,
) -> Result<Lease, Error> {
    let grants = BTreeSet::from([permission]);
    let policy = manager.admit(identity.into(), grants.clone(), grants.clone(), &grants)?;
    policy.authorize(grants, timeout.min(Duration::from_secs(3600)))?;
    policy.begin(
        Budget {
            calls,
            resource_bytes: bytes.saturating_add(8192),
            timeout,
        },
        Arc::new(AtomicBool::new(false)),
    )
}
