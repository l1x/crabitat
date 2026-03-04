use crate::db::*;
use crate::error::ApiError;
use crate::types::*;
use axum::{
    Json,
    extract::State,
};
use crabitat_core::{RunStatus, TaskStatus, now_ms};
use std::collections::HashMap;
use tracing::info;

pub(crate) async fn get_status(State(state): State<AppState>) -> Result<Json<StatusSnapshot>, ApiError> {
    info!("db: building status snapshot");
    let db = state.db.lock().await;
    Ok(Json(build_status_snapshot(&db)?))
}

pub(crate) fn build_status_snapshot(conn: &rusqlite::Connection) -> Result<StatusSnapshot, ApiError> {
    let repos = query_repos(conn)?;
    let crabs = query_crabs(conn)?;
    let missions = query_missions(conn)?;
    let tasks = query_tasks(conn)?;
    let runs = query_runs(conn)?;

    let completed_runs =
        runs.iter().filter(|run| run.status == RunStatus::Completed).collect::<Vec<_>>();

    let avg_end_to_end_ms = if completed_runs.is_empty() {
        None
    } else {
        let sum: u64 =
            completed_runs.iter().map(|run| run.metrics.end_to_end_ms.unwrap_or_default()).sum();
        Some(sum / completed_runs.len() as u64)
    };

    let cached_issue_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM github_issues_cache WHERE state = 'OPEN'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let summary = StatusSummary {
        total_crabs: crabs.len(),
        busy_crabs: crabs.iter().filter(|crab| matches!(crab.state, CrabState::Busy)).count(),
        running_tasks: tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Running))
            .count(),
        running_runs: runs.iter().filter(|run| matches!(run.status, RunStatus::Running)).count(),
        completed_runs: runs
            .iter()
            .filter(|run| matches!(run.status, RunStatus::Completed))
            .count(),
        failed_runs: runs.iter().filter(|run| matches!(run.status, RunStatus::Failed)).count(),
        total_tokens: runs.iter().map(|run| u64::from(run.metrics.total_tokens)).sum(),
        avg_end_to_end_ms,
        cached_issue_count,
    };

    let mut repo_issue_counts: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT repo_id, COUNT(*) FROM github_issues_cache WHERE state = 'OPEN' GROUP BY repo_id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows.flatten() {
            repo_issue_counts.insert(row.0, row.1);
        }
    }

    Ok(StatusSnapshot {
        generated_at_ms: now_ms(),
        summary,
        repos,
        crabs,
        missions,
        tasks,
        runs,
        repo_issue_counts,
    })
}
