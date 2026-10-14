//! A native host owns [`RuntimeOwner`]; UI observers own only [`ControlHandle`].
//! This runs a real pinned Zero Proxy, but does not install a VpnService,
//! PacketTunnelProvider, external TUN or socket-protection adapter.
mod control;
mod owner;

pub use control::ControlHandle;
pub use owner::RuntimeOwner;
pub use zero_api::{
    AuthContext, CommandRequest, CommandResponse, Permission, QueryRequest, QueryResponse,
};
pub use zero_config::RuntimeConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    /// Proxy::spawn returned. Public API supplies no readiness/completion
    /// observer, so this does not assert listener readiness or VPN takeover.
    Spawned,
    Stopping,
    /// Orchestration stopped; native executor disposal is still in progress.
    Disposing,
    /// RunningProxy::shutdown and actual native executor disposal completed.
    Stopped,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub core_instance_id: String,
    pub config_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub identity: Option<Identity>,
    pub issue: Option<Error>,
}
/// Safe client boundary errors deliberately omit config and core error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ExecutorUnavailable,
    InvalidConfiguration,
    UncompiledProtocol,
    NativeTunAdapterUnavailable,
    CoreFailure,
    Forbidden,
    StaleIdentity,
    Busy,
    NotSubmitted,
    /// A submitted request lost its reply. Do not automatically replay.
    ResultUnknown,
    Api(zero_api::ApiErrorCode),
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for Error {}
