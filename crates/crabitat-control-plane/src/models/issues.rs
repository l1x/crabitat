use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Issue {
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub state: String,
    pub fetched_at: String,
}
