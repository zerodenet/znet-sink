//! Configuration workspace: composition, local publication and operation ownership.
//! Kernel execution and database/file adapters retain their existing contracts.
pub(crate) mod apply;
pub(crate) mod capture_apply;
pub(crate) mod composition;
pub(crate) mod dns;
pub(crate) mod dns_migration;
pub(crate) mod dns_rule_sets;
pub(crate) mod mobile_location;
pub(crate) mod native_validation;
pub(crate) mod persistence;
pub(crate) mod profile_editor;
mod report;
pub(crate) mod route_editor;
pub(crate) mod route_integrity;
pub(crate) mod route_rules;
pub(crate) mod rule_migration;
pub(crate) mod rule_references;
pub(crate) mod rules;
pub(crate) mod runtime_network;

pub use report::{CompositionReport, LayerReport};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::models::config_workspace::ConfigTransactionReceipt;

#[derive(Default)]
pub(crate) struct Workspace {
    last_composition: Mutex<Option<CompositionReport>>,
    last_transaction: Mutex<Option<ConfigTransactionReceipt>>,
    next_transaction_id: AtomicU64,
    pub(crate) plans: workspace::Plans,
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

    pub(crate) fn next_transaction_id(&self) -> u64 {
        self.next_transaction_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn record_transaction(&self, receipt: ConfigTransactionReceipt) {
        *self
            .last_transaction
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(receipt);
    }

    pub(crate) fn transaction(&self) -> Option<ConfigTransactionReceipt> {
        self.last_transaction
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests;

pub(crate) mod preferences;

pub(crate) mod local_edits;

pub(crate) mod material;
pub(crate) mod workspace;

pub(crate) mod endpoints;
