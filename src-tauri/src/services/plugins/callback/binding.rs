//! Internal resource identity. It never grants a lease or crosses the guest boundary.
use super::*;
use znet_plugin_sandbox::contract::Capability;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Scope {
    pub(super) key: String,
    fingerprint: String,
}
struct Original {
    scope: Scope,
    review: Review,
    installation_generation: u64,
}
fn request() -> Request {
    Request {
        capability: Capability::BrowserCallback,
        scope: "self".into(),
    }
}
impl Host {
    // Caller holds the owning operation guard and checks the original SDK lease.
    fn callback_original(
        &self,
        manager: &Manager,
        plugin: &str,
        component: &str,
    ) -> AppResult<Original> {
        self.ensure_installation_current(plugin)?;
        let key = format!("{plugin}/{component}");
        let policy = manager
            .component_snapshots()
            .into_iter()
            .find(|row| row.key == key && row.enabled && !row.retired)
            .ok_or_else(stale)?;
        let state = self.state.lock().map_err(|_| stale())?;
        let loaded = state.loaded.get(&key).ok_or_else(stale)?;
        if !loaded.consent.enabled
            || loaded.blocked.is_some()
            || !loaded.consent.grants.contains(&request())
        {
            return Err(stale());
        }
        let review = Review {
            key: key.clone(),
            identity: policy.identity,
            registration: policy.registration,
            revision: policy.revision,
            host_generation: self.platform.review_generation(),
        };
        let installation_generation = loaded.installation_generation;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(&review, installation_generation)).map_err(io::failure)?
            )
        );
        Ok(Original {
            scope: Scope { key, fingerprint },
            review,
            installation_generation,
        })
    }
    fn callback_current(&self, manager: &Manager, original: &Original) -> AppResult<()> {
        if !self.current_review(manager, &original.review)?.enabled {
            return Err(stale());
        }
        let (plugin, _) = original.review.key.split_once('/').ok_or_else(stale)?;
        self.ensure_installation_current(plugin)?;
        if !self
            .state
            .lock()
            .map_err(|_| stale())?
            .loaded
            .get(&original.review.key)
            .is_some_and(|loaded| {
                loaded.installation_generation == original.installation_generation
                    && loaded.consent.enabled
                    && loaded.blocked.is_none()
                    && loaded.consent.grants.contains(&request())
            })
        {
            return Err(stale());
        }
        if !self.current_review(manager, &original.review)?.enabled {
            return Err(stale());
        }
        Ok(())
    }
    pub(in crate::services::plugins) fn callback_create_checked(
        &self,
        manager: &Manager,
        plugin: &str,
        component: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<Created> {
        let _operation = self.operation_guard()?;
        check()?;
        let original = self.callback_original(manager, plugin, component)?;
        self.callbacks.create(original.scope.clone(), &|| {
            self.callback_current(manager, &original)?;
            check()
        })
    }
    pub(in crate::services::plugins) fn callback_poll_checked(
        &self,
        manager: &Manager,
        plugin: &str,
        component: &str,
        id: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<serde_json::Value> {
        let _operation = self.operation_guard()?;
        check()?;
        let original = self.callback_original(manager, plugin, component)?;
        let result = self.callbacks.poll(&original.scope, id)?;
        self.callback_current(manager, &original)?;
        check()?;
        Ok(result)
    }
    pub(in crate::services::plugins) fn callback_cancel_checked(
        &self,
        manager: &Manager,
        plugin: &str,
        component: &str,
        id: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<()> {
        let _operation = self.operation_guard()?;
        check()?;
        let original = self.callback_original(manager, plugin, component)?;
        self.callbacks.cancel(&original.scope, id, &|| {
            self.callback_current(manager, &original)?;
            check()
        })
    }
}

#[cfg(test)]
impl Scope {
    pub(super) fn fixture(key: &str) -> Self {
        Self {
            key: key.into(),
            fingerprint: format!("fixture:{key}"),
        }
    }
}
