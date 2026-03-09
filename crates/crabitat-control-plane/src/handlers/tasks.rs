use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::missions as db_missions;
use crate::db::tasks as db;
use crate::models::tasks::CreateRunRequest;

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

    // 2. Find the mission_id for this task to recalculate mission status
    let mission_id_res: Result<String, String> = conn
        .query_row(
            "SELECT mission_id FROM tasks WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string());

    if let Ok(mid) = mission_id_res {
        let _ = db_missions::recalculate_mission_status(&conn, &mid);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();

    if let Err(e) = db::increment_task_retry(&conn, &task_id) {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))));
    }

    // Recalculate mission status
    let mission_id_res: Result<String, String> = conn
        .query_row(
            "SELECT mission_id FROM tasks WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string());

    if let Ok(mid) = mission_id_res {
        let _ = db_missions::recalculate_mission_status(&conn, &mid);
    }

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
