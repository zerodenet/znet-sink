use std::cell::RefCell;
use znet_client_core::publication::{publish, Publication, PublicationFailure};
struct Store {
    calls: RefCell<Vec<&'static str>>,
    fail: Option<&'static str>,
    rollback_fails: bool,
}
impl Publication for Store {
    type Error = &'static str;
    fn write(&self) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("write");
        if self.fail == Some("write") {
            Err("disk full")
        } else {
            Ok(())
        }
    }
    fn project(&self) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("project");
        if self.fail == Some("project") {
            Err("settings unwritable")
        } else {
            Ok(())
        }
    }
    fn restore(&self) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("restore");
        if self.rollback_fails {
            Err("restore failed")
        } else {
            Ok(())
        }
    }
}
#[test]
fn publication_orders_stores_and_only_allows_memory_commit_after_both_succeed() {
    for (fail, expected) in [
        (None, vec!["write", "project"]),
        (Some("write"), vec!["write"]),
        (Some("project"), vec!["write", "project", "restore"]),
    ] {
        let store = Store {
            calls: RefCell::new(vec![]),
            fail,
            rollback_fails: false,
        };
        let mut installed = false;
        let result = publish(&store).map(|_| {
            installed = true;
        });
        assert_eq!(installed, fail.is_none());
        assert_eq!(result.is_ok(), fail.is_none());
        assert_eq!(*store.calls.borrow(), expected);
    }
}
#[test]
fn projection_and_failed_recovery_are_both_preserved() {
    let store = Store {
        calls: RefCell::new(vec![]),
        fail: Some("project"),
        rollback_fails: true,
    };
    assert!(matches!(
        publish(&store),
        Err(PublicationFailure::Projection {
            error: "settings unwritable",
            recovery: Err("restore failed")
        })
    ));
}
