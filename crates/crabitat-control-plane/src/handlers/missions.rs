use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::missions as db;
use crate::db::repos as repos_db;
use crate::db::settings as settings_db;
use crate::db::tasks as tasks_db;
use crate::mission_service::{AssemblePromptRequest, MissionService};
use crate::models::missions::{CreateMissionRequest, Mission};
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

    // 5. Expand Workflow into Tasks
    for (i, step) in wf.steps.iter().enumerate() {
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

        tasks_db::insert_task(
            &tx,
            &mission.mission_id,
            &step.id,
            i as i64,
            &prompt,
            max_retries,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn setup() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn);
        AppState {
            db: Arc::new(Mutex::new(conn)),
        }
    }

    #[tokio::test]
    async fn test_create_mission_soft_deleted_repo_returns_404() {
        let state = setup();
        let repo_id = {
            let conn = state.db.lock().unwrap();
            let repo = repos_db::insert(&conn, "owner", "name", None, None).unwrap();
            repos_db::delete(&conn, &repo.repo_id).unwrap();
            repo.repo_id
        };

        let req = CreateMissionRequest {
            repo_id,
            issue_number: 1,
            workflow_name: "test-wf".into(),
            flavor_id: None,
        };

        let result = create_mission(State(state), Json(req)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
