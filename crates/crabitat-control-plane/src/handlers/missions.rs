use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::missions as db;
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

pub async fn create_mission(
    State(state): State<AppState>,
    Json(req): Json<CreateMissionRequest>,
) -> Result<(StatusCode, Json<Mission>), (StatusCode, Json<Value>)> {
    let mut conn = state.db.lock().unwrap();

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

        tasks_db::insert_task(&tx, &mission.mission_id, &step.id, i as i64, &prompt)
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

    let tasks = tasks_db::list_tasks_for_mission(&conn, &mission_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(json!({
        "mission": mission,
        "tasks": tasks
    })))
}
