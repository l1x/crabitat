use crate::db::*;
use crate::error::ApiError;
use crate::types::*;
use crate::workflows::{WorkflowRegistry, SyncResult, assemble_workflows, sync_toml_workflows_to_db};
use axum::{
    Json,
    extract::{Path, State},
};
use kiters::eid::ExternalId;
use crabitat_core::now_ms;
use rusqlite::params;
use tracing::info;

pub(crate) async fn list_db_workflows(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkflowRecord>>, ApiError> {
    info!("db: listing workflows");
    let db = state.db.lock().await;
    Ok(Json(query_workflows(&db)?))
}

pub(crate) async fn get_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<WorkflowRecord>, ApiError> {
    info!(workflow_id = %workflow_id, "db: fetching workflow");
    let db = state.db.lock().await;
    let wf = fetch_workflow(&db, &workflow_id)?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;
    Ok(Json(wf))
}

pub(crate) async fn create_workflow(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<Json<WorkflowRecord>, ApiError> {
    info!(name = %request.name, steps = request.steps.len(), "db: creating workflow");
    if request.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    if request.steps.is_empty() {
        return Err(ApiError::bad_request("at least one step is required"));
    }

    let workflow_id = ExternalId::new("wf").to_string();
    let now = now_ms();
    let include_json = serde_json::to_string(&request.include.unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());

    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO workflows (workflow_id, name, description, include, version, source, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            workflow_id,
            request.name,
            request.description.as_deref().unwrap_or(""),
            include_json,
            request.version.as_deref().unwrap_or("1.0.0"),
            "manual",
            now as i64,
        ],
    )?;

    insert_workflow_steps(&db, &workflow_id, &request.steps)?;

    let wf = fetch_workflow(&db, &workflow_id)?
        .ok_or_else(|| ApiError::internal("failed to reload workflow after creation"))?;
    Ok(Json(wf))
}

pub(crate) async fn update_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(request): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowRecord>, ApiError> {
    info!(workflow_id = %workflow_id, "db: updating workflow");
    let db = state.db.lock().await;

    let existing = fetch_workflow(&db, &workflow_id)?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    let name = request.name.unwrap_or(existing.name);
    let description = request.description.unwrap_or(existing.description);
    let include = request.include.unwrap_or(existing.include);
    let include_json = serde_json::to_string(&include).unwrap_or_else(|_| "[]".to_string());
    let version = request.version.unwrap_or(existing.version);

    db.execute(
        "UPDATE workflows SET name = ?2, description = ?3, include = ?4, version = ?5 WHERE workflow_id = ?1",
        params![workflow_id, name, description, include_json, version],
    )?;

    if let Some(steps) = request.steps {
        db.execute("DELETE FROM workflow_steps WHERE workflow_id = ?1", params![workflow_id])?;
        insert_workflow_steps(&db, &workflow_id, &steps)?;
    }

    let wf = fetch_workflow(&db, &workflow_id)?
        .ok_or_else(|| ApiError::internal("failed to reload workflow after update"))?;
    Ok(Json(wf))
}

pub(crate) async fn sync_workflows(
    State(state): State<AppState>,
) -> Result<Json<SyncResult>, ApiError> {
    // Reload registry from current prompts_path
    let prompts_path = {
        let wf = state.workflows.read().unwrap();
        wf.prompts_path.clone()
    };
    let mut new_registry = WorkflowRegistry::load(&prompts_path);
    info!(count = new_registry.manifests.len(), path = %prompts_path.display(), "reloaded workflow registry for sync");

    // Re-assemble workflows from repo stacks
    let db = state.db.lock().await;
    let all_stacks = query_all_repo_stacks(&db)?;
    assemble_workflows(&mut new_registry, all_stacks);

    // Update in-memory registry and clone for DB sync
    let registry_clone = {
        let mut wf = state.workflows.write().unwrap();
        *wf = new_registry;
        wf.clone()
    };

    // Sync to DB
    let result = sync_toml_workflows_to_db(&db, &registry_clone);
    Ok(Json(result))
}

pub(crate) async fn delete_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!(workflow_id = %workflow_id, "db: deleting workflow");
    let db = state.db.lock().await;
    let _existing = fetch_workflow(&db, &workflow_id)?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    db.execute("DELETE FROM workflow_steps WHERE workflow_id = ?1", params![workflow_id])?;
    db.execute("DELETE FROM workflows WHERE workflow_id = ?1", params![workflow_id])?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
