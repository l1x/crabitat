use serde::{Deserialize, Serialize};

/// Represents a workflow defined in a TOML file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFile {
    pub workflow: WorkflowInfo,
    pub steps: Vec<WorkflowStepFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepFile {
    pub id: String,
    pub prompt_file: String,
    pub depends_on: Option<Vec<String>>,
    pub on_fail: Option<String>,
    pub max_retries: Option<u32>,
}

/// DB-backed flavor for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFlavor {
    pub flavor_id: String,
    pub workflow_name: String,
    pub name: String,
    pub prompt_paths: Vec<String>, // JSON array in DB
}

/// The unified view returned by the API (Workflow + its Flavors)
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowDetail {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub steps: Vec<WorkflowStepFile>,
    pub flavors: Vec<WorkflowFlavor>,
}

/// Simplified view for listing all workflows
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub name: String,
    pub description: String,
    pub step_count: usize,
    pub flavor_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateFlavorRequest {
    pub name: String,
    pub prompt_paths: Vec<String>,
}
