use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use std::collections::{HashMap, VecDeque};

use crate::AppState;
use crate::db::missions as db;
use crate::db::repos as repos_db;
use crate::db::settings as settings_db;
use crate::db::tasks as tasks_db;
use crate::mission_service::{AssemblePromptRequest, MissionService};
use crate::models::missions::{CreateMissionRequest, Mission};
use crate::models::workflows::WorkflowStepFile;
use crate::workflow_registry::WorkflowRegistry;

pub async fn list_missions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Mission>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::list_all(&conn) {
        Ok(missions) => Ok(Json(missions)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn list_repo_missions(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<Mission>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::list_by_repo(&conn, &repo_id) {
        Ok(missions) => Ok(Json(missions)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn create_mission(
    State(state): State<AppState>,
    Json(req): Json<CreateMissionRequest>,
) -> Result<(StatusCode, Json<Mission>), (StatusCode, Json<Value>)> {
    let mut conn = state.db.lock().unwrap();

    // Guard: reject missions for soft-deleted repos
    match repos_db::get_by_id(&conn, &req.repo_id) {
        Ok(Some(repo)) if repo.deleted_at.is_some() => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "repo not found"})),
            ));
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "repo not found"})),
            ));
        }
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))));
        }
        Ok(Some(_)) => {}
    }

    // 1. Define Intent (Deterministic Branch)
    let branch = format!("mission/issue-{}", req.issue_number);

    // 2. Initialize Service
    let service = MissionService::new(&conn)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    let prompts_root = settings_db::get(&conn, "prompts_root")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or((
            StatusCode::FAILED_DEPENDENCY,
            Json(json!({"error": "prompts_root not set"})),
        ))?;

    let registry = WorkflowRegistry::new(prompts_root);
    let wf = registry.get_workflow(&req.workflow_name).ok_or((
        StatusCode::NOT_FOUND,
        Json(json!({"error": "workflow not found"})),
    ))?;

    // 3. Start Transaction
    let tx = conn.transaction().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    // 4. Create Mission Record
    let mission = db::insert_mission(&tx, &req, &branch)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // Seed initial state history entry
    db::insert_state_history_entry(&tx, &mission.mission_id, "pending")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // 5. Expand Workflow into Tasks (DAG-aware ordering)
    let step_orders = compute_step_orders(&wf.steps)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;

    for (step_idx, order) in &step_orders {
        let step = &wf.steps[*step_idx];
        let prompt = service
            .assemble_prompt(
                &tx,
                AssemblePromptRequest {
                    workflow_name: &req.workflow_name,
                    step_id: &step.id,
                    flavor_id: req.flavor_id.as_deref(),
                    repo_id: &req.repo_id,
                    issue_number: req.issue_number,
                    context: None, // Initial mission creation has no prior context
                },
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

        let max_retries = step.max_retries.unwrap_or(3) as i64;
        let status = if *order == 0 { "queued" } else { "blocked" };

        tasks_db::insert_task(
            &tx,
            &mission.mission_id,
            &step.id,
            *order as i64,
            &prompt,
            max_retries,
            status,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    }

    // 6. Commit
    tx.commit().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok((StatusCode::CREATED, Json(mission)))
}

pub async fn get_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();

    let mission = db::get_mission(&conn, &mission_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "mission not found"})),
        ))?;

    let mut tasks = tasks_db::list_tasks_for_mission(&conn, &mission_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // Hydrate tasks with their runs
    let mut tasks_with_runs = Vec::new();
    for task in tasks.drain(..) {
        let runs = tasks_db::list_runs_for_task(&conn, &task.task_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

        let mut task_val = json!(task);
        task_val["runs"] = json!(runs);
        tasks_with_runs.push(task_val);
    }

    let state_history = db::get_state_history(&conn, &mission_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(json!({
        "mission": mission,
        "tasks": tasks_with_runs,
        "state_history": state_history
    })))
}

/// Topological sort using Kahn's algorithm.
/// Returns a vec of (step_index, depth) pairs where depth is the DAG level.
pub fn topological_sort_steps(steps: &[WorkflowStepFile]) -> Result<Vec<(usize, usize)>, String> {
    let id_to_idx: HashMap<&str, usize> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();

    let n = steps.len();
    let mut in_degree = vec![0usize; n];
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];

    for (i, step) in steps.iter().enumerate() {
        if let Some(deps) = &step.depends_on {
            for dep_id in deps {
                let &parent = id_to_idx.get(dep_id.as_str()).ok_or_else(|| {
                    format!("step '{}' depends on unknown step '{}'", step.id, dep_id)
                })?;
                children[parent].push(i);
                in_degree[i] += 1;
            }
        }
    }

    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut depth = vec![0usize; n];

    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut visited = 0usize;
    let mut result: Vec<(usize, usize)> = Vec::with_capacity(n);

    while let Some(idx) = queue.pop_front() {
        visited += 1;
        result.push((idx, depth[idx]));

        for &child in &children[idx] {
            in_degree[child] -= 1;
            depth[child] = depth[child].max(depth[idx] + 1);
            if in_degree[child] == 0 {
                queue.push_back(child);
            }
        }
    }

    if visited != n {
        return Err("cycle detected in workflow step dependencies".to_string());
    }

    Ok(result)
}

/// Compute step_order values for workflow steps.
/// If no step has `depends_on`, falls back to sequential enumerate (backward compat).
/// Otherwise uses topological sort to assign DAG depth as step_order.
pub fn compute_step_orders(steps: &[WorkflowStepFile]) -> Result<Vec<(usize, usize)>, String> {
    let has_deps = steps.iter().any(|s| s.depends_on.is_some());

    if !has_deps {
        return Ok(steps.iter().enumerate().map(|(i, _)| (i, i)).collect());
    }

    topological_sort_steps(steps)
}
