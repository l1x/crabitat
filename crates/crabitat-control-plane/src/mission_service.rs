use crate::db::issues as issues_db;
use crate::db::settings as settings_db;
use crate::db::workflows as wf_db;
use crate::workflow_registry::WorkflowRegistry;
use rusqlite::Connection;

pub struct MissionService {
    registry: WorkflowRegistry,
}

impl MissionService {
    pub fn new(conn: &Connection) -> Result<Self, String> {
        let prompts_root = settings_db::get(conn, "prompts_root")
            .map_err(|e| e.to_string())?
            .ok_or("prompts_root not configured")?;

        Ok(Self {
            registry: WorkflowRegistry::new(prompts_root),
        })
    }

    /// Assemble the full prompt for a specific workflow step + flavor + issue
    pub fn assemble_prompt(
        &self,
        conn: &Connection,
        workflow_name: &str,
        step_id: &str,
        flavor_id: Option<&str>,
        repo_id: &str,
        issue_number: i64,
    ) -> Result<String, String> {
        // 1. Get Base Layer (Workflow Step)
        let wf = self
            .registry
            .get_workflow(workflow_name)
            .ok_or_else(|| format!("workflow not found: {}", workflow_name))?;

        let step =
            wf.steps.iter().find(|s| s.id == step_id).ok_or_else(|| {
                format!("step {} not found in workflow {}", step_id, workflow_name)
            })?;

        let base_layer = self.registry.read_prompt(&step.prompt_file)?;

        // 2. Get Flavor Layer
        let mut flavor_layer = String::new();
        if let Some(fid) = flavor_id {
            let flavors = wf_db::list_flavors_for_workflow(conn, workflow_name)?;
            let flavor = flavors
                .iter()
                .find(|f| f.flavor_id == fid)
                .ok_or_else(|| format!("flavor not found: {}", fid))?;

            for path in &flavor.prompt_paths {
                let content = self.registry.read_prompt(path)?;
                flavor_layer.push_str(&content);
                flavor_layer.push_str("\n\n");
            }
        }

        // 3. Get Issue Layer
        let issue = issues_db::get_cached_issue(conn, repo_id, issue_number)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("issue #{} not found in cache", issue_number))?;

        let issue_body = issue.body.unwrap_or_default();
        let issue_layer = format!(
            "<issue>\n  <title>{}</title>\n  <body>\n{}\n  </body>\n</issue>",
            issue.title, issue_body
        );

        // 4. Final Assembly
        let final_prompt = format!(
            "# Instructions\n{}\n\n# Context & Standards\n{}\n\n# Target Issue\n{}",
            base_layer, flavor_layer, issue_layer
        );

        Ok(final_prompt)
    }
}
