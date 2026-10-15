use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub binding: String,
    pub profile_id: String,
    pub config_revision: String,
    pub core_instance_id: String,
    pub scope_generation: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub binding: Option<Binding>,
    pub profile: Option<Profile>,
    pub reason: Option<String>,
}
