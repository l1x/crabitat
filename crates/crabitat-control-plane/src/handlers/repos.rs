use crate::db::*;
use crate::error::ApiError;
use crate::types::*;
use crate::workflows::{assemble_workflows, assembled_name, sync_toml_workflows_to_db};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use crabitat_core::{Mission, MissionStatus, now_ms};
use kiters::eid::ExternalId;
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path as StdPath;
use tracing::info;

use crate::scheduler::{activate_next_mission_in_repo, run_scheduler_tick_db};

pub(crate) async fn create_repo(
    State(state): State<AppState>,
    Json(request): Json<CreateRepoRequest>,
) -> Result<Json<RepoRecord>, ApiError> {
    info!(owner = %request.owner, name = %request.name, "db: creating repo");
    if request.owner.trim().is_empty() || request.name.trim().is_empty() {
        return Err(ApiError::bad_request("owner and name are required"));
    }
    if request.local_path.trim().is_empty() {
        return Err(ApiError::bad_request("local_path is required"));
    }

    let repo_id = ExternalId::new("repo").to_string();
    let default_branch = request.default_branch.unwrap_or_else(|| "main".to_string());
    let language = request.language.unwrap_or_default();
    let stacks = request.stacks.unwrap_or_default();
    let stacks_json =
        serde_json::to_string(&stacks).unwrap_or_else(|_| "[]".to_string());
    let created_at_ms = now_ms();
    let full_name = format!("{}/{}", request.owner, request.name);

    let row = RepoRecord {
        repo_id: repo_id.clone(),
        owner: request.owner,
        name: request.name,
        full_name,
        default_branch,
        language,
        local_path: request.local_path,
        stacks,
        created_at_ms,
    };

    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO repos (repo_id, owner, name, default_branch, language, local_path, stacks, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![row.repo_id, row.owner, row.name, row.default_branch, row.language, row.local_path, stacks_json, row.created_at_ms as i64],
    )?;

    // Re-assemble workflows if stacks were provided
    if !row.stacks.is_empty() {
        let all_stacks = query_all_repo_stacks(&db)?;
        let mut wf = state.workflows.write().unwrap();
        assemble_workflows(&mut wf, all_stacks);
        sync_toml_workflows_to_db(&db, &wf);
    }

    Ok(Json(row))
}

pub(crate) async fn list_repos(State(state): State<AppState>) -> Result<Json<Vec<RepoRecord>>, ApiError> {
    info!("db: listing repos");
    let db = state.db.lock().await;
    Ok(Json(query_repos(&db)?))
}

pub(crate) async fn get_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepoRecord>, ApiError> {
    info!(repo_id = %repo_id, "db: fetching repo");
    let db = state.db.lock().await;
    let repo = fetch_repo(&db, &repo_id)?.ok_or_else(|| ApiError::not_found("repo not found"))?;
    Ok(Json(repo))
}

pub(crate) async fn update_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Json(request): Json<UpdateRepoRequest>,
) -> Result<Json<RepoRecord>, ApiError> {
    info!(repo_id = %repo_id, "db: updating repo");
    let db = state.db.lock().await;

    let existing =
        fetch_repo(&db, &repo_id)?.ok_or_else(|| ApiError::not_found("repo not found"))?;

    let default_branch = request.default_branch.unwrap_or(existing.default_branch);
    let local_path = request.local_path.unwrap_or(existing.local_path);
    let stacks_changed = request.stacks.is_some();
    let stacks = request.stacks.unwrap_or(existing.stacks);
    let stacks_json =
        serde_json::to_string(&stacks).unwrap_or_else(|_| "[]".to_string());

    db.execute(
        "UPDATE repos SET default_branch = ?2, local_path = ?3, stacks = ?4 WHERE repo_id = ?1",
        params![repo_id, default_branch, local_path, stacks_json],
    )?;

    let updated = RepoRecord {
        repo_id,
        owner: existing.owner,
        name: existing.name,
        full_name: existing.full_name,
        default_branch,
        language: existing.language,
        local_path,
        stacks,
        created_at_ms: existing.created_at_ms,
    };

    // Re-assemble workflows when stacks change
    if stacks_changed {
        let all_stacks = query_all_repo_stacks(&db)?;
        let mut wf = state.workflows.write().unwrap();
        assemble_workflows(&mut wf, all_stacks);
        sync_toml_workflows_to_db(&db, &wf);
    }

    Ok(Json(updated))
}

pub(crate) async fn delete_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!(repo_id = %repo_id, "db: deleting repo");
    let db = state.db.lock().await;

    let _existing =
        fetch_repo(&db, &repo_id)?.ok_or_else(|| ApiError::not_found("repo not found"))?;

    db.execute("DELETE FROM repos WHERE repo_id = ?1", params![repo_id])?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Repo languages (from disk via git ls-files)
// ---------------------------------------------------------------------------

fn ext_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "ts" | "tsx" => Some("TypeScript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "astro" => Some("Astro"),
        "toml" => Some("TOML"),
        "yaml" | "yml" => Some("YAML"),
        "json" => Some("JSON"),
        "css" => Some("CSS"),
        "scss" | "sass" => Some("SCSS"),
        "html" | "htm" => Some("HTML"),
        "md" | "markdown" => Some("Markdown"),
        "py" => Some("Python"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "rb" => Some("Ruby"),
        "sh" | "bash" | "zsh" | "fish" => Some("Shell"),
        "sql" => Some("SQL"),
        "graphql" | "gql" => Some("GraphQL"),
        "xml" => Some("XML"),
        "svg" => Some("SVG"),
        "lock" => Some("Lock file"),
        "c" => Some("C"),
        "cpp" | "cc" | "cxx" => Some("C++"),
        "h" | "hpp" => Some("C/C++ Header"),
        "swift" => Some("Swift"),
        "kt" | "kts" => Some("Kotlin"),
        "lua" => Some("Lua"),
        "zig" => Some("Zig"),
        "nix" => Some("Nix"),
        _ => None,
    }
}

fn detect_languages(local_path: &str) -> HashMap<String, u64> {
    let output = match std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(local_path)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lang_bytes: HashMap<String, u64> = HashMap::new();

    for file in stdout.split('\0').filter(|f| !f.is_empty()) {
        let path = StdPath::new(local_path).join(file);
        let size = match std::fs::metadata(&path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if let Some(ext) = StdPath::new(file).extension().and_then(|e| e.to_str())
            && let Some(lang) = ext_to_language(ext)
        {
            *lang_bytes.entry(lang.to_string()).or_default() += size;
        }
    }

    lang_bytes
}

pub(crate) async fn get_repo_languages(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<HashMap<String, u64>>, ApiError> {
    info!(repo_id = %repo_id, "detecting repo languages");
    let local_path = {
        let db = state.db.lock().await;
        let repo = fetch_repo(&db, &repo_id)?
            .ok_or_else(|| ApiError::not_found("repo not found"))?;
        repo.local_path
    };

    let languages = tokio::task::spawn_blocking(move || detect_languages(&local_path))
        .await
        .map_err(|e| ApiError::internal(format!("language detection failed: {e}")))?;

    Ok(Json(languages))
}

// ---------------------------------------------------------------------------
// Issues & Queue
// ---------------------------------------------------------------------------

pub(crate) async fn list_repo_issues(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<GitHubIssueRecord>>, ApiError> {
    info!(repo_id = %repo_id, "db+github: listing repo issues");
    let repo = {
        let db = state.db.lock().await;
        fetch_repo(&db, &repo_id)?.ok_or_else(|| ApiError::not_found("repo not found"))?
    };

    let force_refresh = query.refresh.unwrap_or(false);
    let full_name = format!("{}/{}", repo.owner, repo.name);

    // Try cache first (unless force refresh)
    let issues = if !force_refresh {
        let db = state.db.lock().await;
        if let Some((cached, fetched_at)) = read_cached_issues(&db, &repo_id) {
            let age = now_ms().saturating_sub(fetched_at);
            if age < ISSUES_CACHE_TTL_MS {
                info!(repo_id = %repo_id, age_ms = age, "serving issues from cache");
                cached
            } else {
                drop(db);
                let fresh = state.github.list_issues(&full_name).await?;
                let db = state.db.lock().await;
                write_issues_cache(&db, &repo_id, &fresh)?;
                fresh
            }
        } else {
            drop(db);
            let fresh = state.github.list_issues(&full_name).await?;
            let db = state.db.lock().await;
            write_issues_cache(&db, &repo_id, &fresh)?;
            fresh
        }
    } else {
        info!(repo_id = %repo_id, "force-refreshing issues from GitHub");
        let fresh = state.github.list_issues(&full_name).await?;
        let db = state.db.lock().await;
        write_issues_cache(&db, &repo_id, &fresh)?;
        fresh
    };

    // Build already_queued set from active missions
    let queued_numbers: std::collections::HashSet<i64> = {
        let db = state.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT DISTINCT github_issue_number FROM missions \
             WHERE repo_id = ?1 AND github_issue_number IS NOT NULL \
             AND status NOT IN ('completed', 'failed')",
        )?;
        stmt.query_map(params![repo_id], |row| row.get::<_, i64>(0))?
            .filter_map(Result::ok)
            .collect()
    };

    let records: Vec<GitHubIssueRecord> = issues
        .into_iter()
        .map(|issue| {
            let already_queued = queued_numbers.contains(&issue.number);
            GitHubIssueRecord {
                already_queued,
                number: issue.number,
                title: issue.title,
                body: issue.body,
                labels: issue.labels,
                state: issue.state,
            }
        })
        .collect();

    Ok(Json(records))
}

pub(crate) async fn queue_issue(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Json(request): Json<QueueIssueRequest>,
) -> Result<Json<MissionRecord>, ApiError> {
    info!(repo_id = %repo_id, issue_number = request.issue_number, "db+github: queuing issue");
    // Phase 1: Validate repo and get full_name (brief lock)
    let full_name = {
        let db = state.db.lock().await;
        let (owner, name): (String, String) = db
            .query_row(
                "SELECT owner, name FROM repos WHERE repo_id = ?1",
                params![repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| ApiError::not_found("repo not found"))?;
        format!("{owner}/{name}")
    };

    // Phase 2: Fetch issue details from GitHub (no lock held)
    let detail = state.github.get_issue(&full_name, request.issue_number).await?;

    // Phase 3: All DB work in a single transaction
    let row = {
        let mut db = state.db.lock().await;
        let wf = state.workflows.read().unwrap();
        let tx = db.transaction().map_err(ApiError::from)?;

        // Check if issue is already queued
        let already_queued: i64 = tx.query_row(
            "SELECT COUNT(*) FROM missions WHERE repo_id = ?1 AND github_issue_number = ?2",
            params![repo_id, request.issue_number],
            |row| row.get(0),
        )?;
        if already_queued > 0 {
            return Err(ApiError::bad_request("issue is already queued in this repo"));
        }

        // Compute queue position
        let max_pos: Option<i64> = tx
            .query_row(
                "SELECT MAX(queue_position) FROM missions WHERE repo_id = ?1",
                params![repo_id],
                |row| row.get(0),
            )
            .unwrap_or(None);
        let queue_position = max_pos.unwrap_or(0) + 1;

        let workflow_name = request.workflow.unwrap_or_else(|| {
            // Look up repo stacks to compute the default assembled workflow name
            let stacks_json: String = tx
                .query_row(
                    "SELECT stacks FROM repos WHERE repo_id = ?1",
                    params![repo_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "[]".to_string());
            let mut stacks = parse_stacks_json(&stacks_json);
            if stacks.is_empty() {
                "develop-feature".to_string()
            } else {
                stacks.sort();
                stacks.dedup();
                assembled_name("develop-feature", &stacks)
            }
        });
        let prompt =
            format!("{}#{}: {}\n\n{}", full_name, request.issue_number, detail.title, detail.body);
        let mission = Mission::new(&prompt);
        let row = MissionRecord {
            mission_id: mission.id.to_string(),
            repo_id: repo_id.clone(),
            prompt,
            workflow_name: Some(workflow_name),
            status: MissionStatus::Pending,
            worktree_path: None,
            queue_position: Some(queue_position),
            github_issue_number: Some(request.issue_number),
            github_pr_number: None,
            created_at_ms: mission.created_at_ms,
        };

        tx.execute(
            "INSERT INTO missions (mission_id, repo_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.mission_id,
                row.repo_id,
                row.prompt,
                row.workflow_name,
                mission_status_to_db(row.status),
                row.worktree_path,
                row.queue_position,
                row.github_issue_number,
                row.github_pr_number,
                row.created_at_ms
            ],
        )?;

        activate_next_mission_in_repo(&tx, &repo_id, &wf)?;

        run_scheduler_tick_db(&tx)?;
        tx.commit().map_err(ApiError::from)?;
        row
    };

    Ok(Json(row))
}

pub(crate) async fn list_queue(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<MissionRecord>>, ApiError> {
    info!(repo_id = %repo_id, "db: listing queue");
    let db = state.db.lock().await;

    let mut stmt = db.prepare(
        "SELECT mission_id, repo_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions WHERE repo_id = ?1 AND queue_position IS NOT NULL ORDER BY queue_position ASC",
    )?;
    let rows = stmt.query_map(params![repo_id], |row| {
        Ok(MissionRecord {
            mission_id: row.get(0)?,
            repo_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        })
    })?;

    Ok(Json(rows.filter_map(Result::ok).collect()))
}

pub(crate) async fn remove_from_queue(
    State(state): State<AppState>,
    Path((repo_id, mission_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!(repo_id = %repo_id, mission_id = %mission_id, "db: removing from queue");
    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    // Only allow removing pending missions
    let status: String = tx
        .query_row(
            "SELECT status FROM missions WHERE mission_id = ?1 AND repo_id = ?2 AND queue_position IS NOT NULL",
            params![mission_id, repo_id],
            |row| row.get(0),
        )
        .map_err(|_| ApiError::not_found("mission not found in queue"))?;

    if status != "pending" {
        return Err(ApiError::bad_request("can only remove pending missions from queue"));
    }

    tx.execute("DELETE FROM missions WHERE mission_id = ?1", params![mission_id])?;
    tx.commit().map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "ok": true, "deleted": mission_id })))
}
