use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crabitat_control_plane::AppState;
use crabitat_control_plane::db;
use crabitat_control_plane::db::repos as repos_db;
use crabitat_control_plane::handlers::missions::create_mission;
use crabitat_control_plane::models::missions::CreateMissionRequest;
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
async fn test_create_mission_soft_deleted_repo_returns_404() {
    let state = setup();
    let repo_id = {
        let conn = state.db.lock().unwrap();
        let repo = repos_db::insert(&conn, "owner", "name", None, None).unwrap();
        repos_db::delete(&conn, &repo.repo_id).unwrap();
        repo.repo_id
    };

    let req = CreateMissionRequest {
        repo_id,
        issue_number: 1,
        workflow_name: "test-wf".into(),
        flavor_id: None,
    };

    let result = create_mission(State(state), Json(req)).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, StatusCode::NOT_FOUND);
}
