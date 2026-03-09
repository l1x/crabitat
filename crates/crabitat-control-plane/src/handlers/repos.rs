use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::repos;
use crate::models::{CreateRepoRequest, Repo, UpdateRepoRequest};

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
        Ok(Some(repo)) if repo.deleted_at.is_some() => {
            Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))))
        }
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

pub async fn update_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Json(body): Json<UpdateRepoRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match repos::update(
        &conn,
        &repo_id,
        body.local_path.as_deref(),
        body.repo_url.as_deref(),
    ) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
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
    async fn test_get_repo_soft_deleted_returns_404() {
        let state = setup();
        let repo_id = {
            let conn = state.db.lock().unwrap();
            let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();
            repos::delete(&conn, &repo.repo_id).unwrap();
            repo.repo_id
        };

        let result = get_repo(State(state), Path(repo_id)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_repo_twice_returns_404() {
        let state = setup();
        let repo_id = {
            let conn = state.db.lock().unwrap();
            let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();
            repo.repo_id
        };

        // First delete: success
        let res1 = delete_repo(State(state.clone()), Path(repo_id.clone())).await;
        assert_eq!(res1.unwrap(), StatusCode::NO_CONTENT);

        // Second delete: 404
        let res2 = delete_repo(State(state), Path(repo_id)).await;
        assert!(res2.is_err());
        let (status, _) = res2.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_repos_excludes_deleted() {
        let state = setup();
        {
            let conn = state.db.lock().unwrap();
            repos::insert(&conn, "owner", "active", None, None).unwrap();
            let r2 = repos::insert(&conn, "owner", "deleted", None, None).unwrap();
            repos::delete(&conn, &r2.repo_id).unwrap();
        }

        let Json(repos) = list_repos(State(state)).await.unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "active");
    }
}
