use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    pub repo_id: String,
    pub owner: String,
    pub name: String,
    pub local_path: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub owner: String,
    pub name: String,
    pub local_path: String,
}
