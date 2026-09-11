use std::{collections::VecDeque, sync::Mutex};
use znet_engine_client::{
    configuration::RuntimeIdentity,
    flow_cleanup::{close_previous, FlowControl},
};
struct Control {
    identities: Mutex<VecDeque<Result<&'static str, &'static str>>>,
    closed: Mutex<Vec<String>>,
    fail_close: bool,
}
impl FlowControl for Control {
    type Error = &'static str;
    async fn identity(&self) -> Result<RuntimeIdentity, Self::Error> {
        self.identities
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
            .map(|id| RuntimeIdentity {
                core_instance_id: id.into(),
                config_revision: 1,
            })
    }
    async fn close(&self, id: &str) -> Result<(), Self::Error> {
        self.closed.lock().unwrap().push(id.into());
        if self.fail_close {
            Err("denied")
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn reused_flow_ids_are_never_closed_after_instance_change_or_unreadable_identity() {
    for next in [Ok("new"), Err("offline")] {
        let control = Control {
            identities: Mutex::new([Ok("old"), next].into()),
            closed: Mutex::new(vec![]),
            fail_close: false,
        };
        let report = close_previous(
            &control,
            "old",
            &["one".into(), "reused".into(), "third".into()],
        )
        .await;
        assert_eq!(*control.closed.lock().unwrap(), vec!["one"]);
        assert_eq!(report.closed, 1);
        assert_eq!(report.skipped, 2);
        assert_eq!(report.instance_changed, next.is_ok());
        assert_eq!(report.boundary_error.is_some(), next.is_err());
    }
}
#[tokio::test]
async fn a_close_failure_does_not_hide_remaining_work_and_empty_boundary_does_no_io() {
    let control = Control {
        identities: Mutex::new([Ok("old"), Ok("old")].into()),
        closed: Mutex::new(vec![]),
        fail_close: true,
    };
    let empty = close_previous(&control, "old", &[]).await;
    assert_eq!(empty.closed, 0);
    let report = close_previous(&control, "old", &["one".into(), "two".into()]).await;
    assert_eq!(report.failures.len(), 2);
    assert_eq!(report.skipped, 0);
}
