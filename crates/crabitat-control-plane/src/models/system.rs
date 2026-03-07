use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub gh_installed: bool,
    pub gh_auth_status: bool,
    pub gh_version: Option<String>,
    pub gh_user: Option<String>,
}
