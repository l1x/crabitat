use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::settings as db;
use crate::models::settings::{Setting, UpdateSettingRequest};

pub async fn list_settings(
    State(state): State<AppState>,
) -> Result<Json<Vec<Setting>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::list_all(&conn) {
        Ok(settings) => Ok(Json(settings)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

pub async fn get_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Setting>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::get_full(&conn, &key) {
        Ok(Some(setting)) => Ok(Json(setting)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "setting not found"})),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

pub async fn update_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateSettingRequest>,
) -> Result<Json<Setting>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match db::set(&conn, &key, &body.value) {
        Ok(_) => match db::get_full(&conn, &key) {
            Ok(Some(setting)) => Ok(Json(setting)),
            Ok(None) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "setting not found after upsert"})),
            )),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )),
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}
