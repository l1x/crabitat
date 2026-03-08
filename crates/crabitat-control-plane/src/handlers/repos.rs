use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::repos;
use crate::models::{CreateRepoRequest, Repo};

pub async fn create_repo(
    State(state): State<AppState>,
    Json(body): Json<CreateRepoRequest>,
) -> Result<(StatusCode, Json<Repo>), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::insert(
        &conn,
        &body.owner,
        &body.name,
        body.local_path.as_deref(),
        body.repo_url.as_deref(),
    ) {
        Ok(repo) => Ok((StatusCode::CREATED, Json(repo))),
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e})))),
    }
}

pub async fn list_repos(
    State(state): State<AppState>,
) -> Result<Json<Vec<Repo>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::list(&conn) {
        Ok(repos) => Ok(Json(repos)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn get_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Repo>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::get_by_id(&conn, &repo_id) {
        Ok(Some(repo)) => Ok(Json(repo)),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn delete_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::delete(&conn, &repo_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}
