//! Zero observation semantics. Transports bind one concrete connection; no GUI,
//! platform lookup, process management, or automatic command replay lives here.

mod observation;
mod subscription;

pub use observation::{FlowFilter, ObservationClient, ObservationError, QueryTransport, Snapshot};
pub use subscription::{Binding, Endpoint, Subscription, SubscriptionOwner};

pub mod configuration;

pub mod flow_cleanup;
