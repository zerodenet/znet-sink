//! Native subscription caller policy. The executor remains generic HTTP.
use crate::network::MAX_BODY_BYTES;
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Error, Lease, Manager, Permission};
/// Existing subscription user intent permits HTTP(S) redirects. This broad grant is
/// established by native subscription code, never accepted from guest JSON.
pub fn begin(manager: &Manager) -> Result<Lease, Error> {
    web_read(manager, "builtin.subscription", MAX_BODY_BYTES)
}

pub fn web_read(manager: &Manager, identity: &str, bytes: usize) -> Result<Lease, Error> {
    let grants = BTreeSet::from([Permission::new("network.get", "*")]);
    let policy = manager.admit(identity.into(), grants.clone(), grants.clone(), &grants)?;
    policy.authorize(grants, Duration::from_secs(30))?;
    policy.begin(
        Budget {
            calls: 1,
            resource_bytes: bytes + 8192,
            timeout: Duration::from_secs(30),
        },
        Arc::new(AtomicBool::new(false)),
    )
}
