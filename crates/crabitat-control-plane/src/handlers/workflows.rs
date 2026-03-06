use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::workflows as wf_db;
use crate::models::{
    CreateFlavorRequest, CreateWorkflowRequest, UpdateWorkflowRequest, WorkflowDetail,
    WorkflowFlavor, WorkflowSummary,
};

pub async fn create_workflow(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Json(body): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowDetail>), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    let desc = body.description.as_deref().unwrap_or("");
    match wf_db::insert(&conn, &repo_id, &body.name, desc, &body.steps) {
        Ok(detail) => Ok((StatusCode::CREATED, Json(detail))),
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e})))),
    }
}

pub async fn list_repo_workflows(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<WorkflowSummary>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::list_by_repo(&conn, &repo_id) {
        Ok(list) => Ok(Json(list)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn list_all_workflows(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkflowSummary>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::list_all(&conn) {
        Ok(list) => Ok(Json(list)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn get_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<WorkflowDetail>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::get_detail(&conn, &workflow_id) {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn update_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(body): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowDetail>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::update(
        &conn,
        &workflow_id,
        body.name.as_deref(),
        body.description.as_deref(),
        body.steps.as_deref(),
    ) {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn delete_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::delete(&conn, &workflow_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn create_flavor(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(body): Json<CreateFlavorRequest>,
) -> Result<(StatusCode, Json<WorkflowFlavor>), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::insert_flavor(&conn, &workflow_id, &body.name, body.context.as_deref()) {
        Ok(flavor) => Ok((StatusCode::CREATED, Json(flavor))),
        Err(e) if e.contains("not found") => {
            Err((StatusCode::NOT_FOUND, Json(json!({"error": e}))))
        }
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e})))),
    }
}

pub async fn delete_flavor(
    State(state): State<AppState>,
    Path((_workflow_id, flavor_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::delete_flavor(&conn, &flavor_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}
