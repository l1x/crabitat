use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub mission_id: String,
    pub step_id: String,
    pub step_order: i64,
    pub assembled_prompt: String,
    pub status: String,
    pub retry_count: i64,
    pub max_retries: i64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitInfo {
    pub repo_url: String,
    pub branch: String,
    pub local_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskWithGit {
    pub task: Task,
    pub git: GitInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    pub run_id: String,
    pub task_id: String,
    pub status: String,
    pub logs: Option<String>,
    pub summary: Option<String>,
    pub duration_ms: Option<i64>,
    pub tokens_used: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub status: String,
    pub logs: Option<String>,
    pub summary: Option<String>,
    pub duration_ms: Option<i64>,
    pub tokens_used: Option<i64>,
}
