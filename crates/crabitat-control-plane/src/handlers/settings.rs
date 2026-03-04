use crate::error::ApiError;
use crate::types::AppState;
use crate::workflows::{WorkflowRegistry, sync_toml_workflows_to_db};
use axum::{
    Json,
    extract::State,
};
use rusqlite::params;
use std::path::PathBuf;
use tracing::info;

pub(crate) async fn get_settings(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.db.lock().await;
    let mut stmt = db.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    })?;
    let mut map = serde_json::Map::new();
    for row in rows.flatten() {
        map.insert(row.0, serde_json::Value::String(row.1));
    }
    Ok(Json(serde_json::Value::Object(map)))
}

pub(crate) async fn patch_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let obj = body.as_object().ok_or_else(|| ApiError::bad_request("expected JSON object"))?;

    {
        let db = state.db.lock().await;
        for (key, value) in obj {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            db.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, val_str],
            )?;
        }
    }

    // If prompts_path changed, reload WorkflowRegistry and sync to DB
    if let Some(serde_json::Value::String(new_path)) = obj.get("prompts_path") {
        let path = PathBuf::from(new_path);
        let new_registry = WorkflowRegistry::load(&path);
        info!(count = new_registry.manifests.len(), path = %path.display(), "reloaded workflow registry after settings change");
        let registry_clone = {
            let mut wf = state.workflows.write().unwrap();
            *wf = new_registry;
            wf.clone()
        };
        let db = state.db.lock().await;
        let sync = sync_toml_workflows_to_db(&db, &registry_clone);
        info!(synced = sync.synced, removed = sync.removed, "auto-synced workflows after prompts_path change");
    }

    get_settings(State(state)).await
}
