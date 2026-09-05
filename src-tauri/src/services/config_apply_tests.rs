use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;

struct Fake {
    identities: Mutex<VecDeque<AppResult<queries::KernelRuntimeIdentity>>>,
    result: AppResult<queries::KernelRuntimeIdentity>,
    submissions: Mutex<usize>,
}
impl Backend for Fake {
    async fn identity(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        self.identities
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected identity read")
    }
    async fn submit(&self) -> AppResult<queries::KernelRuntimeIdentity> {
        *self.submissions.lock().unwrap() += 1;
        self.result.clone()
    }
}
fn id(core: &str, revision: u64) -> queries::KernelRuntimeIdentity {
    queries::KernelRuntimeIdentity {
        core_instance_id: core.into(),
        config_revision: revision,
    }
}
fn fixture(
    result: AppResult<queries::KernelRuntimeIdentity>,
    current: queries::KernelRuntimeIdentity,
) -> Fake {
    Fake {
        identities: Mutex::new(VecDeque::from([Ok(id("a", 1)), Ok(current)])),
        result,
        submissions: Mutex::new(0),
    }
}

#[tokio::test]
async fn only_the_confirmed_live_revision_can_reach_local_commit() {
    for (applied, current, accepted) in [
        (id("a", 2), id("a", 2), true),
        (id("b", 1), id("b", 1), false),
        (id("a", 2), id("a", 3), false),
        (id("a", 2), id("b", 1), false),
    ] {
        let backend = fixture(Ok(applied), current);
        let mut committed = false;
        let result = async {
            confirm(&backend).await?;
            committed = true;
            Ok::<_, AppError>(())
        }
        .await;
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(committed, accepted);
        assert_eq!(*backend.submissions.lock().unwrap(), 1);
    }
}

#[tokio::test]
async fn rejection_or_lost_reply_never_retries_or_commits() {
    for code in [
        "core_error",
        "timeout",
        "connection_closed",
        "config_apply_uncertain",
    ] {
        let backend = fixture(
            Err(AppError {
                code,
                message: "injected failure".into(),
                details: None,
            }),
            id("a", 2),
        );
        assert_eq!(confirm(&backend).await.unwrap_err().code, code);
        assert_eq!(*backend.submissions.lock().unwrap(), 1);
        assert_eq!(backend.identities.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn unavailable_preflight_does_not_submit_configuration() {
    let backend = fixture(Ok(id("a", 2)), id("a", 2));
    backend.identities.lock().unwrap()[0] = Err(AppError::internal("unavailable"));
    assert!(confirm(&backend).await.is_err());
    assert_eq!(*backend.submissions.lock().unwrap(), 0);
}
