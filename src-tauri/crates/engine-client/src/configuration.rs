//! Configuration confirmation semantics; no transport discovery or retry.
use std::future::Future;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub core_instance_id: String,
    pub config_revision: u64,
}

pub trait ConfigBackend: Send + Sync {
    type Error: Send;
    fn identity(&self) -> impl Future<Output = Result<RuntimeIdentity, Self::Error>> + Send;
    fn submit(&self) -> impl Future<Output = Result<RuntimeIdentity, Self::Error>> + Send;
}
#[derive(Debug)]
pub enum ApplyFailure<E> {
    Preflight(E),
    Submission(E),
    Confirmation(E),
    IdentityChanged,
}
/// The only successful output of confirmation. Persistence belongs to the
/// application and may begin only after receiving this receipt.
#[derive(Debug)]
pub struct ConfirmedConfig {
    identity: RuntimeIdentity,
}
impl ConfirmedConfig {
    pub fn identity(&self) -> &RuntimeIdentity {
        &self.identity
    }
}
pub async fn confirm<B: ConfigBackend>(
    backend: &B,
) -> Result<ConfirmedConfig, ApplyFailure<B::Error>> {
    let before = backend.identity().await.map_err(ApplyFailure::Preflight)?;
    let applied = backend.submit().await.map_err(ApplyFailure::Submission)?;
    let current = backend
        .identity()
        .await
        .map_err(ApplyFailure::Confirmation)?;
    if before.core_instance_id != applied.core_instance_id || current != applied {
        return Err(ApplyFailure::IdentityChanged);
    }
    Ok(ConfirmedConfig { identity: current })
}
