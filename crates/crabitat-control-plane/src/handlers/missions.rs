use crate::db::*;
use crate::error::ApiError;
use crate::scheduler::{expand_workflow_into_tasks, run_scheduler_tick_db};
use crate::types::*;
use axum::{
    Json,
    extract::{Path, State},
};
use crabitat_core::{Mission, MissionStatus, TaskId, TaskStatus, now_ms};
use rusqlite::params;
use tracing::info;

pub(crate) async fn create_mission(
    State(state): State<AppState>,
    Json(request): Json<CreateMissionRequest>,
) -> Result<Json<MissionRecord>, ApiError> {
    info!(repo_id = %request.repo_id, workflow = ?request.workflow, "db: creating mission");
    if request.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    if request.repo_id.trim().is_empty() {
        return Err(ApiError::bad_request("repo_id is required"));
    }

    let row = {
        let mut db = state.db.lock().await;
        let wf = state.workflows.read().unwrap();
        let tx = db.transaction().map_err(ApiError::from)?;

        let repo_exists: i64 = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM repos WHERE repo_id = ?1)",
            params![request.repo_id],
            |row| row.get(0),
        )?;
        if repo_exists == 0 {
            return Err(ApiError::not_found("repo_id not found"));
        }

        let mission = Mission::new(&request.prompt);
        let row = MissionRecord {
            mission_id: mission.id.to_string(),
            repo_id: request.repo_id,
            prompt: mission.prompt,
            workflow_name: request.workflow.clone(),
            status: MissionStatus::Pending,
            worktree_path: None,
            queue_position: None,
            github_issue_number: None,
            github_pr_number: None,
            created_at_ms: mission.created_at_ms,
        };

        tx.execute(
            "INSERT INTO missions (mission_id, repo_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.mission_id,
                row.repo_id,
                row.prompt,
                row.workflow_name,
                mission_status_to_db(row.status),
                row.worktree_path,
                row.queue_position,
                row.github_issue_number,
                row.github_pr_number,
                row.created_at_ms
            ],
        )?;

        // If a workflow is specified, expand it into tasks
        if let Some(ref workflow_name) = request.workflow {
            let manifest = wf
                .get(workflow_name)
                .ok_or_else(|| {
                    ApiError::not_found(format!("workflow '{workflow_name}' not found"))
                })?
                .clone();

            let worktree_path = format!("burrows/mission-{}", row.mission_id);
            tx.execute(
                "UPDATE missions SET status = ?2, worktree_path = ?3 WHERE mission_id = ?1",
                params![
                    row.mission_id,
                    mission_status_to_db(MissionStatus::Running),
                    worktree_path
                ],
            )?;

            expand_workflow_into_tasks(
                &tx,
                &wf,
                &manifest,
                &row.mission_id,
                &request.prompt,
            )?;
        }

        run_scheduler_tick_db(&tx)?;
        tx.commit().map_err(ApiError::from)?;
        row
    };

    Ok(Json(row))
}

pub(crate) async fn list_missions(
    State(state): State<AppState>,
) -> Result<Json<Vec<MissionRecord>>, ApiError> {
    info!("db: listing missions");
    let db = state.db.lock().await;
    let missions = query_missions(&db)?;
    Ok(Json(missions))
}

pub(crate) async fn get_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
) -> Result<Json<MissionRecord>, ApiError> {
    info!(mission_id = %mission_id, "db: fetching mission");
    let db = state.db.lock().await;
    let mission =
        fetch_mission(&db, &mission_id)?.ok_or_else(|| ApiError::not_found("mission not found"))?;
    Ok(Json(mission))
}

pub(crate) async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    info!(mission_id = %request.mission_id, title = %request.title, "db: creating task");
    if request.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    let task = {
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

        let created_at_ms = now_ms();
        let status = request.status.unwrap_or(TaskStatus::Queued);
        let task_id = TaskId::new().to_string();

        tx.execute(
            "
            INSERT INTO tasks (task_id, mission_id, title, assigned_crab_id, status,
                               step_id, prompt, context,
                               created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                task_id,
                request.mission_id,
                request.title,
                request.assigned_crab_id,
                task_status_to_db(status),
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                created_at_ms,
                created_at_ms
            ],
        )?;

        if let Some(ref crab_id) = request.assigned_crab_id {
            tx.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?1",
                params![crab_id, task_id, created_at_ms],
            )?;
        }

        let task = fetch_task(&tx, &task_id)?
            .ok_or_else(|| ApiError::internal("failed to reload task after creation"))?;

        tx.commit().map_err(ApiError::from)?;
        task
    };

    Ok(Json(task))
}

pub(crate) async fn list_tasks(State(state): State<AppState>) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    info!("db: listing tasks");
    let db = state.db.lock().await;
    Ok(Json(query_tasks(&db)?))
}
