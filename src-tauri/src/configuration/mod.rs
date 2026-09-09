//! Configuration workspace: composition, local publication and operation ownership.
//! Kernel execution and database/file adapters retain their existing contracts.
pub(crate) mod apply;
pub(crate) mod composition;
pub(crate) mod dns;
pub(crate) mod persistence;
mod report;
pub(crate) mod route_integrity;
pub(crate) mod rules;

pub use report::{CompositionReport, LayerReport};
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct Workspace {
    last_composition: Mutex<Option<CompositionReport>>,
}
impl Workspace {
    pub(crate) fn record(&self, report: CompositionReport) {
        *self
            .last_composition
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(report);
    }
    pub(crate) fn report(&self) -> Option<CompositionReport> {
        self.last_composition
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests;

pub(crate) mod preferences;
