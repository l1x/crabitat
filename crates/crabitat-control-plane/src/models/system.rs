use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EnvironmentPath {
    pub environment: String,
    pub resource_type: String,
    pub resource_name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SetEnvironmentPathRequest {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub gh_cli: bool,
    pub gh_auth: bool,
    pub gh_installed: bool,
    pub gh_auth_status: bool,
    pub gh_version: Option<String>,
    pub gh_user: Option<String>,
}
