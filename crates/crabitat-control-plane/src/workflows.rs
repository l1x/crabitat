use crate::error::ApiError;
use crabitat_core::{WorkflowManifest, now_ms};
use kiters::eid::ExternalId;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path as StdPath, PathBuf};
use tracing::info;

// ---------------------------------------------------------------------------
// Workflow Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct WorkflowRegistry {
    pub(crate) manifests: HashMap<String, WorkflowManifest>,
    pub(crate) prompts_path: PathBuf,
    /// Maps short stack names (e.g. "rust") to relative prompt paths (e.g. "prompts/stacks/rust.md")
    pub(crate) stack_map: HashMap<String, String>,
}

impl WorkflowRegistry {
    pub(crate) fn load(prompts_path: &StdPath) -> Self {
        let mut manifests = HashMap::new();
        let workflows_dir = prompts_path.join("workflows");

        if workflows_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&workflows_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match toml::from_str::<WorkflowManifest>(&content) {
                            Ok(manifest) => {
                                info!(
                                    name = %manifest.workflow.name,
                                    steps = manifest.steps.len(),
                                    "loaded workflow"
                                );
                                manifests.insert(manifest.workflow.name.clone(), manifest);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    err = %e,
                                    "failed to parse workflow TOML"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                err = %e,
                                "failed to read workflow file"
                            );
                        }
                    }
                }
            }
        }

        // Build stack_map from prompts/stacks/*.md and prompts/pm/*.md
        let mut stack_map = HashMap::new();
        for subdir in &["prompts/stacks", "prompts/pm"] {
            let dir = prompts_path.join(subdir);
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("md") {
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                let relative = format!("{subdir}/{stem}.md");
                                info!(name = stem, path = %relative, "discovered stack prompt");
                                stack_map.insert(stem.to_string(), relative);
                            }
                        }
                    }
                }
            }
        }

        Self { manifests, prompts_path: prompts_path.to_path_buf(), stack_map }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&WorkflowManifest> {
        self.manifests.get(name)
    }

    pub(crate) fn load_prompt_file(&self, prompt_file: &str) -> Result<String, ApiError> {
        let path = self.prompts_path.join(prompt_file);
        std::fs::read_to_string(&path).map_err(|e| {
            ApiError::internal(format!("failed to read prompt file {}: {e}", path.display()))
        })
    }

    /// Resolve short stack names to prompt file paths using the stack_map.
    pub(crate) fn resolve_stacks(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter_map(|name| self.stack_map.get(name).cloned())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Assembled workflows
// ---------------------------------------------------------------------------

/// Compute the assembled workflow name for a base workflow and a sorted stack combo.
pub(crate) fn assembled_name(base: &str, combo: &[String]) -> String {
    format!("{base}/{}", combo.join("+"))
}

/// Assemble workflows by combining each base workflow with each unique stack combo.
pub(crate) fn assemble_workflows(registry: &mut WorkflowRegistry, repo_stacks: Vec<Vec<String>>) {
    // Remove old assembled manifests (names containing '/')
    registry.manifests.retain(|name, _| !name.contains('/'));

    // Collect unique stack combos (sorted, deduped)
    let mut unique_combos: Vec<Vec<String>> = Vec::new();
    for mut stacks in repo_stacks {
        if stacks.is_empty() {
            continue;
        }
        stacks.sort();
        stacks.dedup();
        if !unique_combos.contains(&stacks) {
            unique_combos.push(stacks);
        }
    }

    // Get base workflow names (those without '/')
    let base_names: Vec<String> = registry.manifests.keys().cloned().collect();

    for combo in &unique_combos {
        let resolved_includes = registry.resolve_stacks(combo);
        for base_name in &base_names {
            let manifest = &registry.manifests[base_name];
            let name = assembled_name(base_name, combo);

            // Merge: base workflow includes + resolved stack includes, deduplicated
            let mut merged_includes = manifest.workflow.include.clone();
            for inc in &resolved_includes {
                if !merged_includes.contains(inc) {
                    merged_includes.push(inc.clone());
                }
            }

            let assembled = WorkflowManifest {
                workflow: crabitat_core::WorkflowMeta {
                    name: name.clone(),
                    description: format!(
                        "{} [{}]",
                        manifest.workflow.description,
                        combo.join("+")
                    ),
                    version: manifest.workflow.version.clone(),
                    include: merged_includes.clone(),
                },
                steps: manifest.steps.clone(),
            };

            info!(name = %name, includes = ?merged_includes, "assembled workflow");
            registry.manifests.insert(name, assembled);
        }
    }
}

// ---------------------------------------------------------------------------
// Workflow TOML → DB sync
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub(crate) struct SyncResult {
    pub(crate) synced: usize,
    pub(crate) removed: usize,
    pub(crate) commit_hash: Option<String>,
    pub(crate) errors: Vec<String>,
}

/// Get the git HEAD commit hash for the prompts directory (submodule or repo).
pub(crate) fn get_prompts_commit_hash(prompts_path: &StdPath) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(prompts_path)
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
}

pub(crate) fn sync_toml_workflows_to_db(
    conn: &Connection,
    registry: &WorkflowRegistry,
) -> SyncResult {
    let mut synced = 0usize;
    let mut errors = Vec::new();
    let commit_hash = get_prompts_commit_hash(&registry.prompts_path);

    let manifest_names: Vec<&String> = registry.manifests.keys().collect();

    for (name, manifest) in &registry.manifests {
        let include_json = serde_json::to_string(&manifest.workflow.include)
            .unwrap_or_else(|_| "[]".to_string());

        // Assembled workflows (name contains '/') get source = 'assembled'
        let source = if name.contains('/') { "assembled" } else { "toml" };

        // Upsert workflow by name
        let upsert_result = conn.execute(
            "INSERT INTO workflows (workflow_id, name, description, include, version, source, commit_hash, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(name) DO UPDATE SET
               description = excluded.description,
               include = excluded.include,
               version = excluded.version,
               source = excluded.source,
               commit_hash = excluded.commit_hash",
            params![
                ExternalId::new("wf").to_string(),
                name,
                manifest.workflow.description,
                include_json,
                manifest.workflow.version,
                source,
                commit_hash,
                now_ms() as i64,
            ],
        );

        if let Err(e) = upsert_result {
            errors.push(format!("upsert workflow {name}: {e}"));
            continue;
        }

        // Get the workflow_id for this name (may be new or existing)
        let wf_id: String = match conn.query_row(
            "SELECT workflow_id FROM workflows WHERE name = ?1",
            params![name],
            |row| row.get(0),
        ) {
            Ok(id) => id,
            Err(e) => {
                errors.push(format!("lookup workflow_id for {name}: {e}"));
                continue;
            }
        };

        // Replace steps: delete old, insert new
        if let Err(e) = conn.execute(
            "DELETE FROM workflow_steps WHERE workflow_id = ?1",
            params![wf_id],
        ) {
            errors.push(format!("delete steps for {name}: {e}"));
            continue;
        }

        let mut step_ok = true;
        for (i, step) in manifest.steps.iter().enumerate() {
            let depends_on_json = serde_json::to_string(&step.depends_on)
                .unwrap_or_else(|_| "[]".to_string());
            let include_json = serde_json::to_string(&step.include.clone().unwrap_or_default())
                .unwrap_or_else(|_| "[]".to_string());

            if let Err(e) = conn.execute(
                "INSERT INTO workflow_steps (workflow_id, position, step_id, prompt_file, depends_on, condition, max_retries, include) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    wf_id,
                    i as i64,
                    step.id,
                    step.prompt_file,
                    depends_on_json,
                    step.condition,
                    step.max_retries as i64,
                    include_json,
                ],
            ) {
                errors.push(format!("insert step {} for {name}: {e}", step.id));
                step_ok = false;
                break;
            }
        }

        if step_ok {
            synced += 1;
        }
    }

    // Remove stale toml/assembled workflows not in current registry
    let removed = if manifest_names.is_empty() {
        conn.execute("DELETE FROM workflows WHERE source IN ('toml', 'assembled')", [])
            .unwrap_or(0)
    } else {
        let placeholders: Vec<String> = manifest_names.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "DELETE FROM workflows WHERE source IN ('toml', 'assembled') AND name NOT IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = manifest_names
            .iter()
            .map(|n| n as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(&sql, params.as_slice()).unwrap_or(0)
    };

    info!(synced, removed, errors = errors.len(), commit = ?commit_hash, "workflow sync complete");
    SyncResult { synced, removed, commit_hash, errors }
}
