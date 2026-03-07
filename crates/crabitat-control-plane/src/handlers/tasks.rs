use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};
use serde::Deserialize;

use crate::AppState;
use crate::db::tasks as db;

pub async fn get_next_task(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::get_next_queued_task(&conn) {
        Ok(Some((task, path))) => Ok(Json(json!({
            "task": task,
            "local_path": path
        }))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "no queued tasks"})))),
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
    match db::update_task_status(&conn, &task_id, &body.status) {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}
