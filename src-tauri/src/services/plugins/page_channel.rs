//! Host-owned signed document bindings. Guests never create or refresh reviews.
use super::sdk::invocation_failure;
use super::*;
use serde::Serialize;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use znet_plugin_sandbox::{contract::Error as InvocationError, policy::Lease, sdk::Call};
const LIMIT: usize = 16;
const TTL: Duration = Duration::from_secs(15 * 60);
#[derive(Serialize)]
pub struct PageDocument {
    pub plugin_id: String,
    pub page_id: String,
    pub html: String,
    pub document_review: String,
    pub components: Vec<ComponentView>,
}
#[derive(Clone)]
pub(crate) struct PageBinding {
    pub(crate) plugin_id: String,
    page_id: String,
    digest: String,
    reviews: Vec<Review>,
    materials: Vec<(String, String)>,
    deadline: Instant,
    pub(crate) cancelled: Arc<AtomicBool>,
}
impl PageBinding {
    pub(crate) fn check(&self) -> AppResult<()> {
        if Instant::now() >= self.deadline {
            self.cancelled.store(true, Ordering::Release);
            return Err(invocation_failure(InvocationError::Expired));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(invocation_failure(InvocationError::Cancelled));
        }
        Ok(())
    }
    pub(crate) fn check_lease(&self, lease: &Lease, request: Option<&Request>) -> AppResult<()> {
        self.check()?;
        lease.check(request).map_err(invocation_failure)
    }
}
#[derive(Default)]
pub(super) struct Store {
    documents: Mutex<BTreeMap<String, PageBinding>>,
    closed: AtomicBool,
}
fn same_review(left: &Review, right: &Review) -> bool {
    left.key == right.key
        && left.identity == right.identity
        && left.registration == right.registration
        && left.revision == right.revision
        && left.host_generation == right.host_generation
}
fn digest(html: &str) -> String {
    format!("{:x}", Sha256::digest(html.as_bytes()))
}
impl Store {
    fn insert(&self, binding: PageBinding) -> AppResult<String> {
        let mut documents = self.documents.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(invocation_failure(InvocationError::Cancelled));
        }
        documents.retain(|_, doc| doc.check().is_ok());
        if documents.len() >= LIMIT {
            return Err(invocation_failure(InvocationError::Busy));
        }
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| AppError::internal("无法创建插件页面凭据"))?;
        let token = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        documents.insert(token.clone(), binding);
        Ok(token)
    }
    fn get(&self, token: &str) -> AppResult<PageBinding> {
        if self.closed.load(Ordering::Acquire) {
            return Err(invocation_failure(InvocationError::Cancelled));
        }
        let doc = self
            .documents
            .lock()
            .unwrap()
            .get(token)
            .cloned()
            .ok_or_else(|| invocation_failure(InvocationError::Cancelled))?;
        doc.check()?;
        Ok(doc)
    }
    fn close_all(&self) {
        self.closed.store(true, Ordering::Release);
        let mut documents = self.documents.lock().unwrap();
        for doc in documents.values() {
            doc.cancelled.store(true, Ordering::Release);
        }
        documents.clear();
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    pub(super) fn reopen_empty(&self) {
        let mut documents = self.documents.lock().unwrap();
        for doc in documents.values() {
            doc.cancelled.store(true, Ordering::Release);
        }
        documents.clear();
        self.closed.store(false, Ordering::Release);
    }
    fn close(&self, token: &str) {
        if let Some(doc) = self.documents.lock().unwrap().remove(token) {
            doc.cancelled.store(true, Ordering::Release);
        }
    }
}
impl Host {
    pub fn page_document(
        &self,
        manager: &Manager,
        plugin_id: &str,
        page_id: &str,
    ) -> AppResult<PageDocument> {
        let _operation = self.operation_guard()?;
        self.checked_rescan(manager)?;
        let html = self.page(plugin_id, page_id)?;
        let components = self
            .snapshot(manager)
            .components
            .into_iter()
            .filter(|component| component.plugin_id == plugin_id)
            .collect::<Vec<_>>();
        let reviews = components
            .iter()
            .filter_map(|component| component.review.clone())
            .collect::<Vec<_>>();
        if components.is_empty() {
            return Err(invocation_failure(InvocationError::Revoked));
        }
        let materials = self
            .state
            .lock()
            .unwrap()
            .loaded
            .iter()
            .filter(|(_, loaded)| loaded.component.manifest().plugin_id == plugin_id)
            .map(|(key, loaded)| (key.clone(), loaded.component.digest().to_owned()))
            .collect::<Vec<_>>();
        let token = self.page_documents.insert(PageBinding {
            plugin_id: plugin_id.into(),
            page_id: page_id.into(),
            digest: digest(&html),
            reviews,
            materials,
            deadline: Instant::now() + TTL,
            cancelled: Arc::new(AtomicBool::new(false)),
        })?;
        Ok(PageDocument {
            plugin_id: plugin_id.into(),
            page_id: page_id.into(),
            html,
            document_review: token,
            components,
        })
    }
    pub fn close_page_documents(&self) {
        self.page_documents.close_all();
    }
    pub fn page_close(&self, token: &str) {
        self.page_documents.close(token);
    }
    fn checked_page_binding(
        &self,
        manager: &Manager,
        token: &str,
        review: Option<&Review>,
    ) -> AppResult<PageBinding> {
        let doc = self.page_documents.get(token)?;
        let validation = (|| {
            if digest(&self.page(&doc.plugin_id, &doc.page_id)?) != doc.digest {
                return Err(invocation_failure(InvocationError::Revoked));
            }
            let materials = self
                .state
                .lock()
                .unwrap()
                .loaded
                .iter()
                .filter(|(_, loaded)| loaded.component.manifest().plugin_id == doc.plugin_id)
                .map(|(key, loaded)| (key.clone(), loaded.component.digest().to_owned()))
                .collect::<Vec<_>>();
            if materials != doc.materials {
                return Err(invocation_failure(InvocationError::Revoked));
            }
            let current = manager
                .component_snapshots()
                .into_iter()
                .filter(|component| {
                    !component.retired && materials.iter().any(|(key, _)| key == &component.key)
                })
                .collect::<Vec<_>>();
            if current.len() != doc.reviews.len() {
                return Err(invocation_failure(InvocationError::Revoked));
            }
            for original in &doc.reviews {
                self.current_review(manager, original)
                    .map_err(|_| invocation_failure(InvocationError::Revoked))?;
            }
            if review.is_some_and(|review| {
                !doc.reviews
                    .iter()
                    .any(|original| same_review(original, review))
            }) {
                return Err(invocation_failure(InvocationError::Revoked));
            }
            doc.check()?;
            Ok(())
        })();
        if validation.is_err() {
            self.page_documents.close(token);
        }
        validation?;
        Ok(doc)
    }
    fn rescan_page(&self, manager: &Manager, token: &str) -> AppResult<()> {
        let result = self.checked_rescan(manager);
        if result.is_err() {
            self.page_documents.close(token);
        }
        result
    }
    pub fn page_check(&self, manager: &Manager, token: &str) -> AppResult<()> {
        let _operation = self.operation_guard()?;
        self.rescan_page(manager, token)?;
        self.checked_page_binding(manager, token, None).map(|_| ())
    }
    pub(crate) fn page_binding(
        &self,
        manager: &Manager,
        token: &str,
        review: &Review,
    ) -> AppResult<PageBinding> {
        let _operation = self.operation_guard()?;
        self.rescan_page(manager, token)?;
        self.checked_page_binding(manager, token, Some(review))
    }
    /// Review lookup and admission use the original revision; begin does not
    /// silently reacquire newer permissions after document validation.
    pub(crate) fn page_lease(
        &self,
        manager: &Manager,
        token: &str,
        review: &Review,
        call: &Call,
    ) -> AppResult<(PageBinding, Lease)> {
        super::sdk::validate_page_call(call)?;
        let _operation = self.operation_guard()?;
        self.rescan_page(manager, token)?;
        let doc = self.checked_page_binding(manager, token, Some(review))?;
        let component_id = review
            .key
            .strip_prefix(&format!("{}/", doc.plugin_id))
            .ok_or_else(|| invocation_failure(InvocationError::Cancelled))?;
        let (component, authority) =
            self.sdk_context(manager, &doc.plugin_id, component_id, &call.request)?;
        let timeout = Duration::from_millis(call.budget.timeout_ms)
            .min(doc.deadline.saturating_duration_since(Instant::now()));
        let lease = authority
            .begin_host_call_at_revision(
                &component,
                review.registration,
                review.revision,
                timeout,
                call.budget.max_result_bytes,
                doc.cancelled.clone(),
            )
            .map_err(invocation_failure)?;
        doc.check_lease(&lease, Some(&call.request))?;
        Ok((doc, lease))
    }
}

#[cfg(test)]
#[path = "page_channel_tests.rs"]
mod tests;
