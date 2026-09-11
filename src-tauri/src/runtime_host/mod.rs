//! Desktop managed-runtime owner. Platform child execution stays outside shared domains.
mod logging;
mod monitor;
mod owner;
mod readiness;
mod shutdown;
mod spawn;
mod start;
mod status;
mod stop;
pub(crate) use owner::Host;
pub use owner::HostSnapshot;
use owner::ManagedCoreProcess;
pub use shutdown::shutdown_managed_runtime;
pub use start::{restart, start};
pub(crate) use status::refresh_status;
pub use stop::stop;
pub(crate) use stop::stop_preserving_system_proxy;
pub fn status(
    state: tauri::State<'_, crate::state::app_state::AppState>,
) -> crate::errors::AppResult<crate::models::core_process::CoreProcessStatus> {
    refresh_status(state.inner())
}
