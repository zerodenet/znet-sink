use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use znet_plugin_sandbox::contract::{ConfigurationSchema, Request};
use znet_plugin_sandbox::distribution::package::PageKind;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub key: String,
    pub identity: String,
    pub registration: u64,
    pub revision: u64,
}
#[derive(Serialize)]
pub struct PermissionView {
    pub request: Request,
    pub required: bool,
    pub supported: bool,
    pub granted: bool,
}
#[derive(Serialize)]
pub struct ComponentView {
    pub plugin_id: String,
    pub name: String,
    pub description: String,
    pub component_id: String,
    pub version: String,
    pub publisher: String,
    pub repository: String,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub license: String,
    pub surfaces: Vec<String>,
    pub review: Option<Review>,
    pub permissions: Vec<PermissionView>,
    pub enabled: bool,
    pub permission_review_required: bool,
    pub running: bool,
    pub blocked: Option<String>,
    pub configuration: Option<ConfigurationView>,
}
#[derive(Serialize)]
pub struct ConfigurationView {
    pub schema: ConfigurationSchema,
    pub values: BTreeMap<String, String>,
    pub configured: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub checked: bool,
    pub components: Vec<ComponentView>,
    pub pages: Vec<PageView>,
    pub notices: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct PageView {
    pub plugin_id: String,
    pub id: String,
    pub title: String,
    pub kind: PageKind,
}

#[derive(Clone, Serialize)]
pub struct PermissionChangeView {
    pub component_id: String,
    pub request: Request,
    pub required: bool,
}

#[derive(Clone, Serialize)]
pub struct InstallReview {
    pub plugin_id: String,
    pub current_version: Option<String>,
    pub candidate_version: String,
    pub candidate_digest: String,
    pub publisher: String,
    pub publisher_fingerprint: String,
    pub first_install: bool,
    pub local_trust: bool,
    pub requested_surfaces: Vec<String>,
    pub requires_approval: bool,
    pub added_permissions: Vec<PermissionChangeView>,
    pub removed_permissions: Vec<PermissionChangeView>,
}
