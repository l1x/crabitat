use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{agent::Agent, error::CrabitatError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectManagementVendor {
    #[serde(rename = "beads")]
    Beads,
    #[serde(rename = "noop")]
    Noop,
}

impl Default for ProjectManagementVendor {
    fn default() -> Self {
        ProjectManagementVendor::Noop
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub file_path: String,
    pub file_type: String,
    pub file_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    #[serde(rename = "tobedone")]
    ToBeDone,
    #[serde(rename = "inprogress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    Failed(String), // Error message for failed tasks
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Task {
    pub id: String,
    pub project_management_vendor: ProjectManagementVendor,
    pub project_management_ref: Option<String>,
    pub title: String,
    pub description: String,
    pub agent_id: String,
    pub state: TaskState,
    pub context_files: Vec<ContextFile>,
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Task: {}", self.title)?;
        write!(f, "\n  ID: {}", self.id)?;
        write!(f, "\n  Description: {}", self.description)?;
        write!(f, "\n  Agent: {}", self.agent_id)?;
        write!(f, "\n  Vendor: {:?}", self.project_management_vendor)?;
        write!(f, "\n  State: {:?}", self.state)?;

        if let Some(ref pm_ref) = self.project_management_ref {
            write!(f, "\n  PM Ref: {}", pm_ref)?;
        }

        if !self.context_files.is_empty() {
            write!(f, "\n  Context Files: {}", self.context_files.len())?;
            for (i, ctx_file) in self.context_files.iter().enumerate() {
                write!(
                    f,
                    "\n    {}. {} ({})",
                    i + 1,
                    ctx_file.file_path,
                    ctx_file.file_description
                )?;
            }
        }

        Ok(())
    }
}

impl Task {
    /// Execute this task using the assigned agent
    pub async fn run(&mut self, agent: &Agent) -> Result<(), CrabitatError> {
        self.state = TaskState::InProgress;

        // TODO: Implement actual task execution logic
        // - Load agent's prompt file
        // - Execute tools based on task requirements
        // - Update task state based on outcome

        self.state = TaskState::Completed;
        Ok(())
    }
}
