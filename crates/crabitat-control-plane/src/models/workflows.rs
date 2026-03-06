use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Workflow {
    pub workflow_id: String,
    pub repo_id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub step_id: String,
    pub workflow_id: String,
    pub step_order: i64,
    pub name: String,
    pub prompt_template: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowFlavor {
    pub flavor_id: String,
    pub workflow_id: String,
    pub name: String,
    pub context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowDetail {
    #[serde(flatten)]
    pub workflow: Workflow,
    pub steps: Vec<WorkflowStep>,
    pub flavors: Vec<WorkflowFlavor>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowSummary {
    #[serde(flatten)]
    pub workflow: Workflow,
    pub flavor_count: i64,
    pub repo_owner: String,
    pub repo_name: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<CreateStepInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateStepInput {
    pub name: String,
    pub prompt_template: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub steps: Option<Vec<CreateStepInput>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFlavorRequest {
    pub name: String,
    pub context: Option<String>,
}
