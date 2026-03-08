use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Mission {
    pub mission_id: String,
    pub repo_id: String,
    pub issue_number: i64,
    pub workflow_name: String,
    pub flavor_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMissionRequest {
    pub repo_id: String,
    pub issue_number: i64,
    pub workflow_name: String,
    pub flavor_id: Option<String>,
}
