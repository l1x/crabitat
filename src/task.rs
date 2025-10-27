use std::fmt;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Task {
    pub id: String,
    pub project_management_vendor: ProjectManagementVendor,
    pub project_management_ref: Option<String>,
    pub title: String,
    pub description: String,
    pub agent_id: String,
    pub context_files: Vec<ContextFile>,
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Task: {}", self.title)?;
        write!(f, "\n  ID: {}", self.id)?;
        write!(f, "\n  Description: {}", self.description)?;
        write!(f, "\n  Agent: {}", self.agent_id)?;
        write!(f, "\n  Vendor: {:?}", self.project_management_vendor)?;

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
