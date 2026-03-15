use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::missions as db_missions;
use crate::db::tasks as db;
use crate::mission_service::reassemble_prompt_with_context;
use crate::models::tasks::{CreateRunRequest, RetryTaskRequest};

#[derive(Deserialize)]
pub struct TaskQuery {
    pub worker_id: Option<String>,
}

pub async fn get_next_task(
    State(state): State<AppState>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::get_next_queued_task(&conn, query.worker_id.as_deref()) {
        Ok(Some(task_with_git)) => Ok(Json(json!(task_with_git))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no queued tasks"})),
        )),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<UpdateStatusRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();

    // 1. Update the task status
    if let Err(e) = db::update_task_status(&conn, &task_id, &body.status) {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))));
    }

    // 2. Promote next blocked task when a task completes
    if body.status == "completed"
        && let Ok(Some(completed_task)) = db::get_task(&conn, &task_id)
        && let Ok(Some(next_task)) = db::get_next_task_in_mission(
            &conn,
            &completed_task.mission_id,
            completed_task.step_order,
        )
        && next_task.status == "blocked"
    {
        // Extract output from the completed task's latest run
        let context = db::list_runs_for_task(&conn, &task_id)
            .unwrap_or_default()
            .into_iter()
            .next()
            .and_then(|r| r.logs)
            .unwrap_or_default();

        // Re-assemble prompt with context from prior step
        if let Ok(new_prompt) = reassemble_prompt_with_context(&conn, &next_task, &context) {
            let _ = db::update_task_assembled_prompt(&conn, &next_task.task_id, &new_prompt);
        }

        let _ = db::update_task_status(&conn, &next_task.task_id, "queued");
    }

    // 3. Recalculate mission status
    if let Ok(Some(task)) = db::get_task(&conn, &task_id) {
        let _ = db_missions::recalculate_mission_status(&conn, &task.mission_id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    body: Option<Json<RetryTaskRequest>>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();

    // 1. Fetch task, return 404 if not found
    let task = db::get_task(&conn, &task_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        })?;

    // 2. Validate task is in failed status
    if task.status != "failed" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": format!("task status is '{}', must be 'failed' to retry", task.status)}),
            ),
        ));
    }

    // 3. If context provided, reassemble prompt with human guidance
    if let Some(ctx) = body.and_then(|b| b.context.clone()) {
        let new_prompt = reassemble_prompt_with_context(&conn, &task, &ctx)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
        db::update_task_assembled_prompt(&conn, &task_id, &new_prompt)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
    }

    // 4. Increment retry (resets status to queued, bumps retry_count)
    db::increment_task_retry(&conn, &task_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // 5. Recalculate mission status
    let _ = db_missions::recalculate_mission_status(&conn, &task.mission_id);

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_run(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::insert_run(&conn, &task_id, &body) {
        Ok(run) => Ok((StatusCode::CREATED, Json(json!(run)))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}
