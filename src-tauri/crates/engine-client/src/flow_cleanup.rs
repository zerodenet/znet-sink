//! Source-switch cleanup is scoped to a captured engine instance, never a UI page.
use crate::configuration::RuntimeIdentity;
use std::future::Future;
pub trait FlowControl: Send + Sync {
    type Error: Send;
    fn identity(&self) -> impl Future<Output = Result<RuntimeIdentity, Self::Error>> + Send;
    fn close(&self, flow_id: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub struct CleanupReport<E> {
    pub closed: usize,
    pub skipped: usize,
    pub failures: Vec<(String, E)>,
    pub boundary_error: Option<E>,
    pub instance_changed: bool,
}
pub async fn close_previous<C: FlowControl>(
    control: &C,
    instance: &str,
    flow_ids: &[String],
) -> CleanupReport<C::Error> {
    let mut report = CleanupReport {
        closed: 0,
        skipped: 0,
        failures: vec![],
        boundary_error: None,
        instance_changed: false,
    };
    for (index, flow_id) in flow_ids.iter().enumerate() {
        match control.identity().await {
            Ok(current) if current.core_instance_id == instance => {}
            Ok(_) => {
                report.instance_changed = true;
                report.skipped = flow_ids.len() - index;
                break;
            }
            Err(error) => {
                report.boundary_error = Some(error);
                report.skipped = flow_ids.len() - index;
                break;
            }
        }
        match control.close(flow_id).await {
            Ok(()) => report.closed += 1,
            Err(error) => report.failures.push((flow_id.clone(), error)),
        }
    }
    report
}
