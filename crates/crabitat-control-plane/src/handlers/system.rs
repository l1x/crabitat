use crate::AppState;
use crate::db::settings as settings_db;
use crate::github;
use crate::models::system::SystemStatus;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;

pub async fn get_status() -> Json<SystemStatus> {
    let status = github::check_status().await;
    Json(status)
}

#[derive(Deserialize)]
pub struct DirQuery {
    pub q: String,
}

pub async fn list_dirs(Query(params): Query<DirQuery>) -> Json<Vec<String>> {
    let query = params.q;
    if query.is_empty() {
        return Json(vec![]);
    }

    let path = std::path::Path::new(&query);

    // Determine the directory to search in and the prefix to match
    let (search_dir, prefix) = if path.is_dir() && query.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new("/")),
            path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        )
    };

    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().starts_with(&prefix.to_lowercase()) && !name.starts_with('.')
                {
                    let full_path = entry.path().to_string_lossy().to_string();
                    dirs.push(full_path);
                }
            }
            if dirs.len() >= 10 {
                break;
            } // Limit results
        }
    }

    Json(dirs)
}

pub async fn get_environment_path(
    State(state): State<AppState>,
    Path((env, res_type, res_name)): Path<(String, String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match settings_db::get_environment_path(&conn, &env, &res_type, &res_name) {
        Ok(Some(path)) => Ok(Json(json!({ "path": path }))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "path not found"})),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

pub async fn list_environment_paths(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match settings_db::list_all_environment_paths(&conn) {
        Ok(paths) => Ok(Json(json!(paths))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

#[derive(Deserialize)]
pub struct UpdateEnvPathRequest {
    pub path: String,
}

pub async fn update_environment_path(
    State(state): State<AppState>,
    Path((env, res_type, res_name)): Path<(String, String, String)>,
    Json(body): Json<UpdateEnvPathRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match settings_db::upsert_environment_path(&conn, &env, &res_type, &res_name, &body.path) {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}
