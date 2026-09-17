//! Native subscription caller policy. The executor remains generic HTTP.
use crate::network::MAX_BODY_BYTES;
use std::time::Duration;
use znet_client_core::capability::{Error, Lease, Manager};
/// Existing subscription user intent permits HTTP(S) redirects. This broad grant is
/// established by native subscription code, never accepted from guest JSON.
pub fn begin(manager: &Manager) -> Result<Lease, Error> {
    web_read(manager, "builtin.subscription", MAX_BODY_BYTES)
}

pub fn web_read(manager: &Manager, identity: &str, bytes: usize) -> Result<Lease, Error> {
    crate::host::network(
        manager,
        identity,
        "network.get",
        1,
        bytes,
        Duration::from_secs(30),
    )
}
