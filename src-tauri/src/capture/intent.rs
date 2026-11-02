//! A newly admitted GUI lifetime always starts with capture switched off.
use crate::{errors::AppResult, models::app_config::AppConfig};
use std::path::Path;

pub(crate) fn begin_lifetime(config: &mut AppConfig, path: &Path) -> AppResult<()> {
    let saved = super::shutdown::clear_saved_tun_intent(config, path);
    // A stale ON value from a killed/updater-relaunched GUI must not trigger
    // auto-connect even when the settings file is temporarily unwritable.
    config.tun.enabled = Some(false);
    saved
}

#[cfg(test)]
#[path = "intent_tests.rs"]
mod tests;
