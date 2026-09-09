use super::MultiplexedConnection;
use crate::{errors::AppResult, models::core::CoreEndpoint};
use std::time::Duration;

/// A subscribed connection with a bounded owner lifetime. Startup probes
/// must not publish an unverified peer into the shared connection manager.
pub(crate) struct ScopedConnection(MultiplexedConnection);

impl ScopedConnection {
    pub(crate) fn connect(endpoint: CoreEndpoint, timeout: Duration) -> AppResult<Self> {
        MultiplexedConnection::connect(endpoint, timeout).map(Self)
    }

    pub(crate) fn connection(&self) -> &MultiplexedConnection {
        &self.0
    }
}

impl Drop for ScopedConnection {
    fn drop(&mut self) {
        // The reader thread owns a connection reference too. Merely dropping
        // our reference would leave its blocking read and pipe handle alive.
        self.0.retire();
    }
}
