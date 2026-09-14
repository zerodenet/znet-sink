use super::{Error, Lease};
use std::sync::{Arc, Mutex};
/// Opaque, non-serializable and bound to the exact invocation, not a caller-supplied ID.
/// Dropping it releases the reservation. Revoked/expired invocations cannot extract it.
pub struct Resource<T> {
    value: Option<T>,
    bytes: usize,
    owner: Arc<Mutex<(u32, usize)>>,
}
impl<T> Resource<T> {
    pub(super) fn new(value: T, bytes: usize, owner: Arc<Mutex<(u32, usize)>>) -> Self {
        Self {
            value: Some(value),
            bytes,
            owner,
        }
    }
    pub fn take(mut self, lease: &Lease) -> Result<T, Error> {
        lease.check(None)?;
        if !Arc::ptr_eq(&self.owner, &lease.usage) {
            return Err(Error::PermissionDenied);
        }
        Ok(self.value.take().expect("resource consumed once"))
    }
}
impl<T> Drop for Resource<T> {
    fn drop(&mut self) {
        self.owner.lock().unwrap().1 -= self.bytes;
    }
}
