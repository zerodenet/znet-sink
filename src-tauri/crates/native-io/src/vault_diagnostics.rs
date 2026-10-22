//! Debug-only public facts. No key, account, native exception or credential fields.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub attempted: bool,
    pub os_status: Option<i32>,
}
impl Step {
    fn valid(&self) -> bool {
        self.attempted == self.os_status.is_some()
    }
    fn is(&self, status: i32) -> bool {
        self.attempted && self.os_status == Some(status)
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Admission {
    pub protected_data_available: bool,
    pub probe: Step,
    pub api_available: bool,
}
impl Admission {
    fn valid(&self) -> bool {
        self.probe.valid()
            && (self.protected_data_available || !self.probe.attempted)
            && self.api_available
                == (self.protected_data_available && (self.probe.is(0) || self.probe.is(-25300)))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Scope,
    ProtectedDataGate,
    ReadBeforeCreate,
    Random,
    Create,
    ReadAfterCreate,
    Cleanup,
    Completed,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultSmokeDiagnostics {
    pub schema_version: u32,
    pub isolated: bool,
    pub protected_data_available: bool,
    pub last_host_admission: Option<Admission>,
    pub availability_probe: Step,
    pub read_before_create: Step,
    pub random: Step,
    pub create: Step,
    pub read_after_create: Step,
    pub cleanup: Step,
    pub read_back_matches: Option<bool>,
    pub stop_stage: Stage,
    pub round_trip: bool,
}
impl VaultSmokeDiagnostics {
    pub fn validate(self) -> Result<Self> {
        let steps = [
            &self.availability_probe,
            &self.read_before_create,
            &self.random,
            &self.create,
            &self.read_after_create,
            &self.cleanup,
        ];
        let structural = self.schema_version == 1
            && self.isolated
            && steps.iter().all(|step| step.valid())
            && self
                .last_host_admission
                .as_ref()
                .is_none_or(Admission::valid)
            && self.round_trip == (self.stop_stage == Stage::Completed)
            && self.read_back_matches.is_some() == self.read_after_create.is(0);
        let skipped = || steps.iter().all(|step| !step.attempted);
        let initial = self.availability_probe.attempted && self.read_before_create.attempted;
        let random = initial && self.read_before_create.is(-25300) && self.random.attempted;
        let create = random && self.random.is(0) && self.create.attempted;
        let after = create && self.create.is(0) && self.read_after_create.attempted;
        let read_back = after && self.read_back_matches == Some(true);
        let cleaned = self.cleanup.is(0) || self.cleanup.is(-25300);
        let phased = match self.stop_stage {
            Stage::Scope => skipped(),
            Stage::ProtectedDataGate => !self.protected_data_available && skipped(),
            Stage::ReadBeforeCreate => {
                initial
                    && !self.read_before_create.is(-25300)
                    && !self.random.attempted
                    && !self.create.attempted
                    && !self.read_after_create.attempted
                    && self.cleanup.attempted == self.read_before_create.is(0)
            }
            Stage::Random => {
                random
                    && !self.random.is(0)
                    && !self.create.attempted
                    && !self.read_after_create.attempted
                    && !self.cleanup.attempted
            }
            Stage::Create => {
                create
                    && !self.create.is(0)
                    && !self.read_after_create.attempted
                    && self.cleanup.attempted
            }
            Stage::ReadAfterCreate => after && !read_back && self.cleanup.attempted,
            Stage::Cleanup => read_back && self.cleanup.attempted && !cleaned,
            Stage::Completed => read_back && cleaned,
        };
        if !structural || !phased {
            return Err(Error::InvalidReceipt("invalid_vault_diagnostic_receipt"));
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn gate() -> Value {
        let skipped = json!({"attempted":false,"osStatus":null});
        json!({"schemaVersion":1,"isolated":true,"protectedDataAvailable":false,
            "lastHostAdmission":{"protectedDataAvailable":false,"probe":skipped,"apiAvailable":false},
            "availabilityProbe":skipped,"readBeforeCreate":skipped,"random":skipped,
            "create":skipped,"readAfterCreate":skipped,"cleanup":skipped,
            "readBackMatches":null,"stopStage":"protectedDataGate","roundTrip":false})
    }
    fn step(status: i32) -> Value {
        json!({"attempted":true,"osStatus":status})
    }
    fn positive() -> Value {
        let mut value = gate();
        value["protectedDataAvailable"] = json!(true);
        value["lastHostAdmission"] =
            json!({"protectedDataAvailable":true,"probe":step(-50),"apiAvailable":false});
        value["availabilityProbe"] = step(-50);
        value["readBeforeCreate"] = step(-25300);
        for key in ["random", "create", "readAfterCreate", "cleanup"] {
            value[key] = step(0);
        }
        value["readBackMatches"] = json!(true);
        value["stopStage"] = json!("completed");
        value["roundTrip"] = json!(true);
        value
    }
    fn validate(value: Value) -> Result<VaultSmokeDiagnostics> {
        serde_json::from_value::<VaultSmokeDiagnostics>(value)
            .map_err(|_| Error::InvalidReceipt("invalid_vault_diagnostic_receipt"))?
            .validate()
    }
    #[test]
    fn gate_probe_and_isolated_success_never_rewrite_production_admission() {
        assert!(validate(gate()).is_ok());
        let result = validate(positive()).unwrap();
        assert!(result.round_trip);
        assert!(!result.last_host_admission.unwrap().api_available);
        let mut isolated = positive();
        isolated["protectedDataAvailable"] = json!(false);
        isolated["lastHostAdmission"] = gate()["lastHostAdmission"].clone();
        assert!(validate(isolated).is_ok());
        let mut value = gate();
        value["availabilityProbe"] = step(-34018);
        assert!(validate(value).is_err());
    }
    #[test]
    fn creation_readback_and_cleanup_failures_remain_distinct() {
        for (stage, status) in [
            ("create", -34018),
            ("readAfterCreate", -25300),
            ("cleanup", -25308),
        ] {
            let mut value = positive();
            value[stage] = step(status);
            value["stopStage"] = json!(stage);
            value["roundTrip"] = json!(false);
            if stage == "create" {
                value["readAfterCreate"] = json!({"attempted":false,"osStatus":null});
            }
            if stage != "cleanup" {
                value["readBackMatches"] = Value::Null;
            }
            assert!(validate(value).is_ok(), "{stage}");
        }
        let mut value = positive();
        value["readBackMatches"] = json!(false);
        assert!(validate(value).is_err());
    }
    #[test]
    fn arbitrary_fields_status_without_execution_and_fake_success_are_rejected() {
        for key in ["keyBase64", "account", "message", "url"] {
            let mut value = positive();
            value[key] = json!("secret-canary");
            assert!(validate(value).is_err());
        }
        let mut value = positive();
        value["lastHostAdmission"]["probe"]["attempted"] = json!(false);
        assert!(validate(value).is_err());
        let mut value = gate();
        value["roundTrip"] = json!(true);
        assert!(validate(value).is_err());
    }
}
