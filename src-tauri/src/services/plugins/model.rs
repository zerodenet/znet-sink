use serde::{Deserialize, Serialize};
use znet_plugin_sandbox::contract::Request;

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
    pub component_id: String,
    pub version: String,
    pub publisher: String,
    pub review: Option<Review>,
    pub permissions: Vec<PermissionView>,
    pub enabled: bool,
    pub running: bool,
    pub blocked: Option<String>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub checked: bool,
    pub components: Vec<ComponentView>,
    pub notices: Vec<String>,
}
