use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::{issues as issues_db, repos};
use crate::github;
use crate::models::Issue;

/// GET /v1/repos/{repo_id}/issues — return cached issues, or fetch if cache is empty
pub async fn list_repo_issues(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<Issue>>, (StatusCode, Json<Value>)> {
    let (owner, name) = lookup_repo(&state, &repo_id)?;

    // Return cached if available
    {
        let conn = state.db.lock().unwrap();
        if issues_db::has_cached(&conn, &repo_id).unwrap_or(false) {
            return match issues_db::list_by_repo(&conn, &repo_id) {
                Ok(issues) => Ok(Json(issues)),
                Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
            };
        }
    }

    // No cache — fetch from GitHub
    fetch_and_cache(&state, &repo_id, &owner, &name).await
}

/// POST /v1/repos/{repo_id}/issues/refresh — force re-fetch from GitHub
pub async fn refresh_repo_issues(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<Issue>>, (StatusCode, Json<Value>)> {
    let (owner, name) = lookup_repo(&state, &repo_id)?;
    fetch_and_cache(&state, &repo_id, &owner, &name).await
}

pub fn lookup_repo(
    state: &AppState,
    repo_id: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::get_by_id(&conn, repo_id) {
        Ok(Some(repo)) if repo.deleted_at.is_some() => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "repo not found"})),
        )),
        Ok(Some(repo)) => Ok((repo.owner, repo.name)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "repo not found"})),
        )),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

async fn fetch_and_cache(
    state: &AppState,
    repo_id: &str,
    owner: &str,
    name: &str,
) -> Result<Json<Vec<Issue>>, (StatusCode, Json<Value>)> {
    let issues = github::fetch_issues(owner, name)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;

    let conn = state.db.lock().unwrap();

    // We DO NOT clear the cache anymore, because missions refer to issues.
    // Instead we upsert the ones we found.
    issues_db::upsert_issues(&conn, repo_id, &issues)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    // Mark missing issues as closed (optional, but good for accuracy)
    // For now, we just return the updated list.

    match issues_db::list_by_repo(&conn, repo_id) {
        Ok(issues) => Ok(Json(issues)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}
