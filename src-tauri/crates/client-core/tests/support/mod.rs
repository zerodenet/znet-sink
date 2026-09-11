//! Integration fixture composing host facts and the independently owned tool ledger.
use znet_client_core::probe_jobs::ProbeJobs;
use znet_client_core::*;
pub struct TestHost {
    core: ClientCore,
    jobs: ProbeJobs,
}
impl TestHost {
    pub fn new(id: Option<ProfileId>, rev: ConfigRevision) -> Self {
        Self {
            core: ClientCore::new(id, rev),
            jobs: Default::default(),
        }
    }
    pub fn snapshot(&self) -> ClientCoreSnapshot {
        let mut s = self.core.snapshot();
        s.active_probe_jobs = self
            .jobs
            .list_probe_jobs(None)
            .into_iter()
            .filter(|j| !j.state.is_terminal())
            .collect();
        s
    }
    pub fn start_probe(
        &mut self,
        r: StartProbeRequest,
        t: u64,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        let o = self.jobs.start_probe(&self.core.snapshot().scope, r, t)?;
        if o.created {
            self.core.advance_snapshot();
        }
        Ok(o)
    }
    pub fn get_probe_job(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        self.jobs.get_probe_job(id)
    }
    pub fn record_probe_result(
        &mut self,
        id: ProbeJobId,
        s: &ClientScope,
        r: ProbeTargetResult,
    ) -> Option<ProbeJobSnapshot> {
        let o = self.jobs.record_probe_result(id, s, r);
        self.flush();
        o
    }
    pub fn cancel_probe(&mut self, id: ProbeJobId, t: u64) -> Option<ProbeJobSnapshot> {
        let o = self.jobs.cancel_probe(id, t);
        self.core.advance_snapshot();
        o
    }
    pub fn timeout_probe(&mut self, id: ProbeJobId, t: u64) -> Option<ProbeJobSnapshot> {
        let o = self.jobs.timeout_probe(id, t);
        self.flush();
        o
    }
    pub fn configuration_committed(&mut self, id: Option<ProfileId>, rev: ConfigRevision, t: u64) {
        self.jobs
            .invalidate_active_jobs(ProbeJobState::InvalidatedByConfigChange, t);
        self.core.configuration_committed(id, rev, t);
    }
    pub fn core_instance_started(&mut self, t: u64) {
        self.jobs
            .invalidate_active_jobs(ProbeJobState::InvalidatedByCoreRestart, t);
        self.core.core_instance_started(t);
    }
    pub fn core_instance_lost(&mut self, t: u64) {
        self.jobs
            .invalidate_active_jobs(ProbeJobState::InvalidatedByCoreRestart, t);
        self.core.core_instance_lost(t);
    }
    pub fn record_observation(&mut self, o: ProbeObservation) -> bool {
        self.core.record_observation(o)
    }
    pub fn observations_for_config(&self, s: &ClientScope) -> Vec<ProbeObservation> {
        self.core.observations_for_config(s)
    }
    fn flush(&mut self) {
        for o in self.jobs.take_observations() {
            self.core.record_observation(o);
        }
    }
}
