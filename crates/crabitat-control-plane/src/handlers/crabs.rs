use crate::db::*;
use crate::error::ApiError;
use crate::scheduler::run_scheduler_tick_db;
use crate::types::*;
use axum::{
    Json,
    extract::State,
};
use crabitat_core::now_ms;
use rusqlite::params;
use tracing::info;

pub(crate) async fn register_crab(
    State(state): State<AppState>,
    Json(request): Json<RegisterCrabRequest>,
) -> Result<Json<CrabRecord>, ApiError> {
    info!(crab_id = %request.crab_id, name = %request.name, "db: registering crab");
    if request.crab_id.trim().is_empty()
        || request.repo_id.trim().is_empty()
        || request.name.trim().is_empty()
    {
        return Err(ApiError::bad_request("crab_id, repo_id, and name are required"));
    }

    let crab = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        let repo_exists: i64 = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM repos WHERE repo_id = ?1)",
            params![request.repo_id],
            |row| row.get(0),
        )?;
        if repo_exists == 0 {
            return Err(ApiError::not_found("repo_id not found"));
        }

        let updated_at_ms = now_ms();
        let crab_state = request.state.unwrap_or(CrabState::Idle);

        tx.execute(
            "
            INSERT INTO crabs (crab_id, repo_id, name, state, current_task_id, current_run_id, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)
            ON CONFLICT(crab_id) DO UPDATE SET
              repo_id=excluded.repo_id,
              name=excluded.name,
              state=excluded.state,
              updated_at_ms=excluded.updated_at_ms
            ",
            params![
                request.crab_id,
                request.repo_id,
                request.name,
                crab_state.as_str(),
                updated_at_ms
            ],
        )?;

        let crab = fetch_crab(&tx, &request.crab_id)?
            .ok_or_else(|| ApiError::internal("failed to reload crab after registration"))?;

        // New idle crab available — run scheduler to assign queued tasks
        run_scheduler_tick_db(&tx)?;
        tx.commit().map_err(ApiError::from)?;
        crab
    };

    Ok(Json(crab))
}

pub(crate) async fn list_crabs(State(state): State<AppState>) -> Result<Json<Vec<CrabRecord>>, ApiError> {
    info!("db: listing crabs");
    let db = state.db.lock().await;
    Ok(Json(query_crabs(&db)?))
}
