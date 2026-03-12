use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Mission {
    pub mission_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub issue_number: i64,
    pub workflow_name: String,
    pub flavor_id: Option<String>,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_worker_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub mission_id: String,
    pub state: String,
    pub entered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMissionRequest {
    pub repo_id: String,
    pub issue_number: i64,
    pub workflow_name: String,
    pub flavor_id: Option<String>,
}
