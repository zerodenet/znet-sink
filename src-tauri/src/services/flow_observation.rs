//! Application boundary: select the observation endpoint once, then pass only
//! the immutable binding to the Zero client. Business reads never guess a pipe.
use super::{common::lock, core_config};
use crate::{
    errors::AppResult,
    kernel::{observation::FlowObservation, protocol},
    state::app_state::AppState,
};
use znet_engine_client::Binding;

pub(crate) fn binding(state: &AppState) -> AppResult<Binding> {
    if let Some(binding) = state.observations().binding() {
        return Ok(binding);
    }
    let config = lock(state.app_config(), "app_config")?;
    let options = core_config::ipc_options_from_app_config(&config.core);
    Ok(Binding {
        endpoint: protocol::endpoint_from_options(Some(&options))?,
        timeout: protocol::timeout_from_options(Some(&options))?,
    })
}

pub(crate) async fn connect(state: &AppState) -> AppResult<FlowObservation> {
    FlowObservation::connect(binding(state)?).await
}
