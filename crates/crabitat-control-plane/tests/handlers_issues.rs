use axum::http::StatusCode;

use crabitat_control_plane::AppState;
use crabitat_control_plane::db;
use crabitat_control_plane::db::repos;
use crabitat_control_plane::handlers::issues::lookup_repo;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

fn setup() -> AppState {
    let conn = Connection::open_in_memory().unwrap();
    db::migrate(&conn);
    AppState {
        db: Arc::new(Mutex::new(conn)),
    }
}

#[test]
fn test_lookup_repo_active() {
    let state = setup();
    let repo_id = {
        let conn = state.db.lock().unwrap();
        let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();
        repo.repo_id
    };

    let res = lookup_repo(&state, &repo_id);
    assert!(res.is_ok());
    let (owner, name) = res.unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(name, "name");
}

#[test]
fn test_lookup_repo_soft_deleted_returns_404() {
    let state = setup();
    let repo_id = {
        let conn = state.db.lock().unwrap();
        let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();
        repos::delete(&conn, &repo.repo_id).unwrap();
        repo.repo_id
    };

    let res = lookup_repo(&state, &repo_id);
    assert!(res.is_err());
    let (status, _) = res.unwrap_err();
    assert_eq!(status, StatusCode::NOT_FOUND);
}
