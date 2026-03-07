use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::settings as settings_db;
use crate::db::workflows as wf_db;
use crate::models::workflows::{
    CreateFlavorRequest, WorkflowDetail, WorkflowFlavor, WorkflowSummary,
};
use crate::workflow_registry::WorkflowRegistry;

fn get_registry(
    conn: &rusqlite::Connection,
) -> Result<WorkflowRegistry, (StatusCode, Json<Value>)> {
    match settings_db::get(conn, "prompts_root") {
        Ok(Some(root)) => Ok(WorkflowRegistry::new(root)),
        Ok(None) => {
            tracing::warn!("prompts_root not configured in settings");
            Err((
                StatusCode::FAILED_DEPENDENCY,
                Json(json!({"error": "prompts_root not configured in settings"})),
            ))
        }
        Err(e) => {
            tracing::error!("failed to get prompts_root from db: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ))
        }
    }
}

pub async fn list_all_workflows(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkflowSummary>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    let registry = get_registry(&conn)?;

    let workflows = registry.list_workflows();
    let mut summaries = Vec::new();

    for wf in workflows {
        let flavor_count = match wf_db::count_flavors_for_workflow(&conn, &wf.workflow.name) {
            Ok(count) => count,
            Err(e) => {
                tracing::error!(
                    "failed to count flavors for workflow {}: {}",
                    wf.workflow.name,
                    e
                );
                0
            }
        };

        summaries.push(WorkflowSummary {
            name: wf.workflow.name,
            description: wf.workflow.description,
            step_count: wf.steps.len(),
            flavor_count,
        });
    }

    Ok(Json(summaries))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<WorkflowDetail>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    let registry = get_registry(&conn)?;

    let wf = registry.get_workflow(&name).ok_or_else(|| {
        tracing::warn!("workflow not found in registry: {}", name);
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "workflow not found"})),
        )
    })?;

    let flavors = wf_db::list_flavors_for_workflow(&conn, &name).map_err(|e| {
        tracing::error!("failed to list flavors for workflow {}: {}", name, e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))
    })?;

    Ok(Json(WorkflowDetail {
        name: wf.workflow.name,
        description: wf.workflow.description,
        version: wf.workflow.version,
        steps: wf.steps,
        flavors,
    }))
}

pub async fn create_flavor(
    State(state): State<AppState>,
    Path(workflow_name): Path<String>,
    Json(body): Json<CreateFlavorRequest>,
) -> Result<(StatusCode, Json<WorkflowFlavor>), (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    // Validate workflow exists
    let registry = get_registry(&conn)?;
    if registry.get_workflow(&workflow_name).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "workflow not found"})),
        ));
    }

    match wf_db::insert_flavor(&conn, &workflow_name, &body.name, &body.prompt_paths) {
        Ok(flavor) => Ok((StatusCode::CREATED, Json(flavor))),
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e})))),
    }
}

pub async fn delete_flavor(
    State(state): State<AppState>,
    Path((_workflow_name, flavor_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::delete_flavor(&conn, &flavor_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "flavor not found"})),
        )),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

pub async fn update_flavor(
    State(state): State<AppState>,
    Path((_workflow_name, flavor_id)): Path<(String, String)>,
    Json(body): Json<CreateFlavorRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    match wf_db::update_flavor(&conn, &flavor_id, &body.name, &body.prompt_paths) {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

#[derive(Deserialize)]
pub struct PromptContentRequest {
    pub paths: Vec<String>,
}

pub async fn get_prompts_content(
    State(state): State<AppState>,
    Json(body): Json<PromptContentRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    tracing::info!("fetching prompt content for paths: {:?}", body.paths);
    let conn = state.db.lock().unwrap();
    let registry = get_registry(&conn)?;

    let mut combined = String::new();
    for path in &body.paths {
        match registry.read_prompt(path) {
            Ok(content) => {
                combined.push_str(&format!("--- {} ---\n", path));
                combined.push_str(&content);
                combined.push_str("\n\n");
            }
            Err(e) => {
                tracing::error!("failed to read prompt {}: {}", path, e);
                combined.push_str(&format!("--- ERROR READING {} ---\n{}\n\n", path, e));
            }
        }
    }

    Ok(Json(json!({ "content": combined })))
}

pub async fn list_prompt_files(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<Value>)> {
    let conn = state.db.lock().unwrap();
    let registry = get_registry(&conn)?;
    Ok(Json(registry.list_prompt_files()))
}
