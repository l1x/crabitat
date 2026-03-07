use crate::models::workflows::WorkflowFile;
use std::fs;
use std::path::{Path, PathBuf};

pub struct WorkflowRegistry {
    prompts_root: PathBuf,
}

impl WorkflowRegistry {
    pub fn new<P: AsRef<Path>>(prompts_root: P) -> Self {
        Self {
            prompts_root: prompts_root.as_ref().to_path_buf(),
        }
    }

    /// List all workflows in {prompts_root}/workflows/*.toml
    pub fn list_workflows(&self) -> Vec<WorkflowFile> {
        let workflows_dir = self.prompts_root.join("workflows");
        let mut workflows = Vec::new();

        if let Ok(entries) = fs::read_dir(workflows_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            match toml::from_str::<WorkflowFile>(&content) {
                                Ok(wf) => workflows.push(wf),
                                Err(e) => tracing::error!("failed to parse workflow TOML at {:?}: {}", path, e),
                            }
                        }
                        Err(e) => tracing::error!("failed to read workflow file at {:?}: {}", path, e),
                    }
                }
            }
        }

        workflows
    }

    /// Get a workflow by its name (from the TOML [workflow] name field)
    pub fn get_workflow(&self, name: &str) -> Option<WorkflowFile> {
        self.list_workflows()
            .into_iter()
            .find(|w| w.workflow.name == name)
    }

    /// Recursively list all .md files in the prompts root
    pub fn list_prompt_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        self.walk_prompts(&self.prompts_root, &mut files);
        files
    }

    fn walk_prompts(&self, dir: &Path, files: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip 'workflows' directory as it contains TOMLs, not prompt fragments
                    if path.file_name().and_then(|s| s.to_str()) == Some("workflows") {
                        continue;
                    }
                    self.walk_prompts(&path, files);
                } else if path.extension().and_then(|s| s.to_str()) == Some("md")
                    && let Ok(rel_path) = path.strip_prefix(&self.prompts_root)
                    && let Some(rel_str) = rel_path.to_str()
                {
                    files.push(rel_str.to_string());
                }
            }
        }
    }

    /// Read the content of a prompt file relative to prompts_root
    #[allow(dead_code)]
    pub fn read_prompt(&self, rel_path: &str) -> Result<String, String> {
        let full_path = self.prompts_root.join(rel_path);
        fs::read_to_string(full_path).map_err(|e| e.to_string())
    }
}
