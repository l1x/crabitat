use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub mission_id: String,
    pub step_id: String,
    pub step_order: i64,
    pub assembled_prompt: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Run {
    pub run_id: String,
    pub task_id: String,
    pub status: String,
    pub logs: Option<String>,
    pub summary: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}
