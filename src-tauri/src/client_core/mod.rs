//! Authoritative desktop client state and workflow boundary.
//!
//! This module deliberately has no dependency on Tauri. The desktop shell
//! owns composition and transport; client state, identities, and lifecycle
//! semantics live here so they can be tested without a WebView.

mod application;
mod domain;
mod kernel;

pub use application::ClientCore;
pub use domain::{
    ClientCoreError, ClientCoreSnapshot, ClientScope, ConfigRevision, CoreInstanceId,
    NodeGroupSnapshot, NodeId, NodeObservationSource, NodeScreenSnapshot, NodeSnapshot, PolicyId,
    ProbeJobId, ProbeJobKind, ProbeJobSnapshot, ProbeJobState, ProbeObservation,
    ProbeObservationSource, ProbeTargetResult, ProfileId, SnapshotRevision, SourceStatus,
    StartProbeOutcome, StartProbeRequest,
};
pub use kernel::{
    ClientKernel, KernelNodeSnapshot, KernelProbeKind, KernelProbeRequest, KernelProbeResult,
};
