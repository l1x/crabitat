use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crabitat_control_plane::AppState;
use crabitat_control_plane::db;
use crabitat_control_plane::db::repos;
use crabitat_control_plane::handlers::repos::{delete_repo, get_repo, list_repos};
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

    let res1 = delete_repo(State(state.clone()), Path(repo_id.clone())).await;
    assert_eq!(res1.unwrap(), StatusCode::NO_CONTENT);

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

    let Json(all) = list_repos(State(state)).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "active");
}
