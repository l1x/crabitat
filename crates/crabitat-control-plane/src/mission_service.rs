use crate::db::issues as issues_db;
use crate::db::settings as settings_db;
use crate::db::workflows as wf_db;
use crate::workflow_registry::WorkflowRegistry;
use rusqlite::Connection;

pub struct MissionService {
    registry: WorkflowRegistry,
}

pub struct AssemblePromptRequest<'a> {
    pub workflow_name: &'a str,
    pub step_id: &'a str,
    pub flavor_id: Option<&'a str>,
    pub repo_id: &'a str,
    pub issue_number: i64,
    pub context: Option<&'a str>,
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
        req: AssemblePromptRequest,
    ) -> Result<String, String> {
        // 1. Get Base Layer (Workflow Step)
        let wf = self
            .registry
            .get_workflow(req.workflow_name)
            .ok_or_else(|| format!("workflow not found: {}", req.workflow_name))?;

        let step = wf
            .steps
            .iter()
            .find(|s| s.id == req.step_id)
            .ok_or_else(|| {
                format!(
                    "step {} not found in workflow {}",
                    req.step_id, req.workflow_name
                )
            })?;

        let base_layer = self.registry.read_prompt(&step.prompt_file)?;

        // 2. Get Flavor Layer
        let mut flavor_layer = String::new();
        if let Some(fid) = req.flavor_id {
            let flavors = wf_db::list_flavors_for_workflow(conn, req.workflow_name)?;
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
        let issue = issues_db::get_cached_issue(conn, req.repo_id, req.issue_number)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("issue #{} not found in cache", req.issue_number))?;

        let issue_body = issue.body.clone().unwrap_or_default();
        let issue_layer = format!(
            "<issue>\n  <title>{}</title>\n  <body>\n{}\n  </body>\n</issue>",
            issue.title, issue_body
        );

        // 4. Resolve Template Variables
        // Note: {{worktree_path}} is handled by the Crab worker (late-binding)
        let mission_content = format!("{}\n\n{}", issue.title, issue_body);

        let mut resolved_base = base_layer.replace("{{mission}}", &mission_content);
        let mut resolved_flavor = flavor_layer.replace("{{mission}}", &mission_content);

        // Handle {{context}} cleanup
        let ctx_val = req.context.unwrap_or("");
        if ctx_val.is_empty() {
            resolved_base = resolved_base.replace("{{context}}", "");
            resolved_flavor = resolved_flavor.replace("{{context}}", "");

            // Clean up the "Context from prior steps" header if it exists
            resolved_base = resolved_base.replace("## Context from prior steps", "");
            resolved_flavor = resolved_flavor.replace("## Context from prior steps", "");
        } else {
            resolved_base = resolved_base.replace("{{context}}", ctx_val);
            resolved_flavor = resolved_flavor.replace("{{context}}", ctx_val);
        }

        // 5. Final Assembly
        let final_prompt = format!(
            "# Instructions\n{}\n\n# Context & Standards\n{}\n\n# Target Issue\n{}",
            resolved_base.trim(),
            resolved_flavor.trim(),
            issue_layer
        );

        Ok(final_prompt)
    }
}
