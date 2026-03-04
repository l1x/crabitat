use crate::error::ApiError;
use crate::types::*;
use axum::{
    Json,
    extract::{Query, State},
};
use std::path::Path as StdPath;
use tracing::info;

// ---------------------------------------------------------------------------
// Prompt-file browsing
// ---------------------------------------------------------------------------

pub(crate) async fn list_prompt_files(State(state): State<AppState>) -> Result<Json<Vec<String>>, ApiError> {
    let wf = state.workflows.read().unwrap();
    let base = &wf.prompts_path;
    info!(base = %base.display(), exists = base.exists(), is_dir = base.is_dir(), "listing prompt files");
    let mut files = Vec::new();
    collect_md_files(base, base, &mut files);
    info!(count = files.len(), "prompt files found: {:?}", files);
    files.sort();
    Ok(Json(files))
}

fn collect_md_files(base: &StdPath, dir: &StdPath, result: &mut Vec<String>) {
    info!(dir = %dir.display(), exists = dir.exists(), "scanning directory");
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            info!(dir = %dir.display(), err = %e, "failed to read directory");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_string());
        info!(path = %path.display(), is_dir, ext = ?ext, "found entry");
        if is_dir {
            collect_md_files(base, &path, result);
        } else if ext.as_deref() == Some("md") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "README.md" | "AGENTS.md" | "CLAUDE.md") {
                info!(name, "skipping excluded file");
                continue;
            }
            if let Ok(rel) = path.strip_prefix(base) {
                info!(rel = %rel.display(), "adding prompt file");
                result.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

pub(crate) async fn preview_prompt_file(
    State(state): State<AppState>,
    Query(query): Query<PromptFileQuery>,
) -> Result<Json<PromptFilePreview>, ApiError> {
    info!(path = %query.path, "previewing prompt file");
    let wf = state.workflows.read().unwrap();
    let content = wf.load_prompt_file(&query.path)?;
    Ok(Json(PromptFilePreview { path: query.path, content }))
}

// ---------------------------------------------------------------------------
// Stacks
// ---------------------------------------------------------------------------

pub(crate) async fn list_stacks(State(state): State<AppState>) -> Result<Json<Vec<StackEntry>>, ApiError> {
    let wf = state.workflows.read().unwrap();
    let mut stacks: Vec<StackEntry> = wf
        .stack_map
        .iter()
        .map(|(name, path)| StackEntry { name: name.clone(), path: path.clone() })
        .collect();
    stacks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(stacks))
}

// ---------------------------------------------------------------------------
// Skills discovery
// ---------------------------------------------------------------------------

fn discover_skills(prompts_path: &StdPath) -> Vec<SkillRecord> {
    let mut skills = Vec::new();

    // Scan common Claude skill locations
    let home = dirs::home_dir().unwrap_or_default();
    let scan_dirs = [
        home.join(".claude/commands"),
        home.join(".claude/skills"),
        prompts_path.join("skills"),
    ];

    for dir in &scan_dirs {
        if !dir.is_dir() {
            continue;
        }
        scan_skill_dir(dir, &mut skills);
    }

    skills
}

fn scan_skill_dir(dir: &StdPath, skills: &mut Vec<SkillRecord>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Check for SKILL.md inside subdirectory
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                let description = std::fs::read_to_string(&skill_md)
                    .ok()
                    .and_then(|content| content.lines().next().map(|l| l.trim_start_matches('#').trim().to_string()))
                    .unwrap_or_default();
                skills.push(SkillRecord {
                    name,
                    path: skill_md.display().to_string(),
                    description,
                });
            }
            scan_skill_dir(&path, skills);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            let name = path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let description = std::fs::read_to_string(&path)
                .ok()
                .and_then(|content| content.lines().next().map(|l| l.trim_start_matches('#').trim().to_string()))
                .unwrap_or_default();
            skills.push(SkillRecord {
                name,
                path: path.display().to_string(),
                description,
            });
        }
    }
}

pub(crate) async fn list_skills(State(state): State<AppState>) -> Result<Json<Vec<SkillRecord>>, ApiError> {
    let prompts_path = {
        let wf = state.workflows.read().unwrap();
        wf.prompts_path.clone()
    };

    let skills = tokio::task::spawn_blocking(move || discover_skills(&prompts_path))
        .await
        .map_err(|e| ApiError::internal(format!("skill discovery failed: {e}")))?;

    Ok(Json(skills))
}
