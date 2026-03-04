use crate::db::*;
use crate::error::ApiError;
use crate::scheduler::{cascade_workflow, run_scheduler_tick_db};
use crate::types::*;
use axum::{
    Json,
    extract::State,
};
use crabitat_core::{RunId, RunStatus, TaskStatus, now_ms};
use rusqlite::params;
use tracing::info;

pub(crate) async fn start_run(
    State(state): State<AppState>,
    Json(request): Json<StartRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    info!(task_id = %request.task_id, crab_id = %request.crab_id, "db: starting run");
    if request.burrow_path.trim().is_empty() {
        return Err(ApiError::bad_request("burrow_path is required"));
    }

    let run_id = request.run_id.unwrap_or_else(|| RunId::new().to_string());
    let status = request.status.unwrap_or(RunStatus::Running);
    let now = now_ms();
    let progress = request.progress_message.unwrap_or_else(|| "run started".to_string());

    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    let mission_exists: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM missions WHERE mission_id = ?1)",
        params![request.mission_id],
        |row| row.get(0),
    )?;
    if mission_exists == 0 {
        return Err(ApiError::not_found("mission_id not found"));
    }

    let task_exists: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE task_id = ?1)",
        params![request.task_id],
        |row| row.get(0),
    )?;
    if task_exists == 0 {
        return Err(ApiError::not_found("task_id not found"));
    }

    tx.execute(
        "
        INSERT INTO runs (
          run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
          progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
          first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
          started_at_ms, updated_at_ms, completed_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 0, 0, 0, NULL, NULL, NULL, NULL, ?9, ?10, NULL)
        ",
        params![
            run_id,
            request.mission_id,
            request.task_id,
            request.crab_id,
            run_status_to_db(status),
            request.burrow_path,
            burrow_mode_to_db(request.burrow_mode),
            progress,
            now,
            now
        ],
    )
    .map_err(|err| ApiError::bad_request(format!("failed to start run: {err}")))?;

    tx.execute(
        "UPDATE tasks SET assigned_crab_id = ?1, status = ?2, updated_at_ms = ?3 WHERE task_id = ?4",
        params![request.crab_id, task_status_to_db(TaskStatus::Running), now, request.task_id],
    )?;

    tx.execute(
        "UPDATE crabs SET state = 'busy', current_task_id = ?1, current_run_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?4",
        params![request.task_id, run_id, now, request.crab_id],
    )?;

    let run = fetch_run(&tx, &run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after start"))?;
    tx.commit().map_err(ApiError::from)?;
    Ok(Json(run))
}

pub(crate) async fn update_run(
    State(state): State<AppState>,
    Json(request): Json<UpdateRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    info!(run_id = %request.run_id, status = ?request.status, "db: updating run");
    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    let existing =
        fetch_run(&tx, &request.run_id)?.ok_or_else(|| ApiError::not_found("run_id not found"))?;

    let now = now_ms();
    let status = request.status.unwrap_or(existing.status);
    let progress_message = request.progress_message.unwrap_or(existing.progress_message.clone());
    let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

    tx.execute(
        "
        UPDATE runs
        SET status = ?2,
            progress_message = ?3,
            prompt_tokens = ?4,
            completion_tokens = ?5,
            total_tokens = ?6,
            first_token_ms = ?7,
            llm_duration_ms = ?8,
            execution_duration_ms = ?9,
            end_to_end_ms = ?10,
            updated_at_ms = ?11
        WHERE run_id = ?1
        ",
        params![
            request.run_id,
            run_status_to_db(status),
            progress_message,
            metrics.prompt_tokens,
            metrics.completion_tokens,
            metrics.total_tokens,
            metrics.first_token_ms.map(|v| v as i64),
            metrics.llm_duration_ms.map(|v| v as i64),
            metrics.execution_duration_ms.map(|v| v as i64),
            metrics.end_to_end_ms.map(|v| v as i64),
            now
        ],
    )?;

    match status {
        RunStatus::Running => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Running), now],
            )?;
            tx.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, current_run_id = ?3, updated_at_ms = ?4 WHERE crab_id = ?1",
                params![existing.crab_id, existing.task_id, existing.run_id, now],
            )?;
        }
        RunStatus::Blocked => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Blocked), now],
            )?;
        }
        RunStatus::Completed => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Completed), now],
            )?;
        }
        RunStatus::Failed => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Failed), now],
            )?;
        }
        RunStatus::Queued => {}
    }

    let updated = fetch_run(&tx, &request.run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after update"))?;
    tx.commit().map_err(ApiError::from)?;
    Ok(Json(updated))
}

pub(crate) async fn complete_run(
    State(state): State<AppState>,
    Json(request): Json<CompleteRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    info!(run_id = %request.run_id, status = ?request.status, "db: completing run");
    if !matches!(request.status, RunStatus::Completed | RunStatus::Failed) {
        return Err(ApiError::bad_request(
            "status must be completed or failed for /v1/runs/complete",
        ));
    }

    let run = {
        let mut db = state.db.lock().await;
        let wf = state.workflows.read().unwrap();
        let tx = db.transaction().map_err(ApiError::from)?;

        let existing = fetch_run(&tx, &request.run_id)?
            .ok_or_else(|| ApiError::not_found("run_id not found"))?;

        let completed_at = now_ms();
        let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

        tx.execute(
            "
            UPDATE runs
            SET status = ?2,
                summary = ?3,
                prompt_tokens = ?4,
                completion_tokens = ?5,
                total_tokens = ?6,
                first_token_ms = ?7,
                llm_duration_ms = ?8,
                execution_duration_ms = ?9,
                end_to_end_ms = ?10,
                completed_at_ms = ?11,
                updated_at_ms = ?11
            WHERE run_id = ?1
            ",
            params![
                request.run_id,
                run_status_to_db(request.status),
                request.summary,
                metrics.prompt_tokens,
                metrics.completion_tokens,
                metrics.total_tokens,
                metrics.first_token_ms.map(|v| v as i64),
                metrics.llm_duration_ms.map(|v| v as i64),
                metrics.execution_duration_ms.map(|v| v as i64),
                metrics.end_to_end_ms.map(|v| v as i64),
                completed_at
            ],
        )?;

        let task_status = match request.status {
            RunStatus::Completed => TaskStatus::Completed,
            RunStatus::Failed => TaskStatus::Failed,
            _ => TaskStatus::Running,
        };
        tx.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![existing.task_id, task_status_to_db(task_status), completed_at],
        )?;

        tx.execute(
            "UPDATE crabs SET state = 'idle', current_task_id = NULL, current_run_id = NULL, updated_at_ms = ?2 WHERE crab_id = ?1",
            params![existing.crab_id, completed_at],
        )?;

        let run = fetch_run(&tx, &request.run_id)?
            .ok_or_else(|| ApiError::internal("failed to reload run after completion"))?;

        cascade_workflow(
            &tx,
            &existing.mission_id,
            &existing.task_id,
            &wf,
        )?;

        run_scheduler_tick_db(&tx)?;
        tx.commit().map_err(ApiError::from)?;
        run
    };

    Ok(Json(run))
}
