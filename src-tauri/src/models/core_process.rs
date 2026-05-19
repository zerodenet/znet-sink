use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CoreProcessState {
    NotStarted,
    Starting,
    Running,
    Exited,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreProcessStatus {
    pub state: CoreProcessState,
    pub pid: Option<u32>,
    pub kernel: String,
    pub executable_path: Option<String>,
    pub working_dir: Option<String>,
    pub config_path: Option<String>,
    pub endpoint_path: String,
    pub started_at_unix_ms: Option<u64>,
    pub exited_at_unix_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
}
