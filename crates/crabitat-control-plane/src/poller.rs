use crate::error::ApiError;
use crate::scheduler::{cascade_workflow, run_scheduler_tick_db};
use crate::types::AppState;
use crabitat_core::{RunId, now_ms};
use rusqlite::params;
use std::time::Duration;
use tracing::info;

pub(crate) async fn spawn_merge_wait_poller(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(e) = poll_merge_wait_tasks(&state).await {
            tracing::warn!(err = ?e, "merge-wait poll error");
        }
    }
}

struct MergeWaitPollItem {
    task_id: String,
    mission_id: String,
    pr_number: Option<i64>,
    repo: Option<String>,
}

async fn poll_merge_wait_tasks(state: &AppState) -> Result<(), ApiError> {
    info!("db+github: polling merge-wait tasks");
    // Find merge-wait tasks that are queued
    let tasks_to_poll: Vec<MergeWaitPollItem> = {
        let db = state.db.lock().await;
        let mut stmt = db.prepare(
            "
            SELECT t.task_id, t.mission_id, m.github_pr_number, r.owner || '/' || r.name
            FROM tasks t
            JOIN missions m ON t.mission_id = m.mission_id
            JOIN repos r ON m.repo_id = r.repo_id
            WHERE t.step_id = 'merge-wait' AND t.status = 'queued'
            ",
        )?;
        let rows: Vec<_> = stmt
            .query_map([], |row| {
                Ok(MergeWaitPollItem {
                    task_id: row.get(0)?,
                    mission_id: row.get(1)?,
                    pr_number: row.get(2)?,
                    repo: row.get(3)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        rows
    };

    for item in tasks_to_poll {
        let (Some(pr_num), Some(ref repo)) = (item.pr_number, item.repo) else {
            continue;
        };

        let pr_status = match state.github.get_pr_status(repo, pr_num).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(pr = pr_num, err = ?e, "failed to check PR status");
                continue;
            }
        };

        {
            let mut db = state.db.lock().await;
            let wf = state.workflows.read().unwrap();
            let tx = db.transaction().map_err(ApiError::from)?;
            let now = now_ms();

            if pr_status.state == "MERGED" || pr_status.merged_at.is_some() {
                let run_id = RunId::new().to_string();
                tx.execute(
                    "INSERT INTO runs (run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode, progress_message, summary, prompt_tokens, completion_tokens, total_tokens, started_at_ms, updated_at_ms, completed_at_ms) VALUES (?1, ?2, ?3, 'system', 'completed', '', 'worktree', 'PR merged', ?4, 0, 0, 0, ?5, ?5, ?5)",
                    params![run_id, item.mission_id, item.task_id, format!("PR #{pr_num} merged"), now],
                )?;

                tx.execute(
                    "UPDATE tasks SET status = 'completed', updated_at_ms = ?2 WHERE task_id = ?1",
                    params![item.task_id, now],
                )?;

                cascade_workflow(
                    &tx,
                    &item.mission_id,
                    &item.task_id,
                    &wf,
                )?;

                run_scheduler_tick_db(&tx)?;
                tx.commit().map_err(ApiError::from)?;

                info!(pr = pr_num, mission_id = %item.mission_id, "merge-wait completed: PR merged");
            } else if pr_status.state == "CLOSED" {
                tx.execute(
                    "UPDATE tasks SET status = 'failed', updated_at_ms = ?2 WHERE task_id = ?1",
                    params![item.task_id, now],
                )?;

                cascade_workflow(
                    &tx,
                    &item.mission_id,
                    &item.task_id,
                    &wf,
                )?;

                run_scheduler_tick_db(&tx)?;
                tx.commit().map_err(ApiError::from)?;

                info!(pr = pr_num, mission_id = %item.mission_id, "merge-wait failed: PR closed without merge");
            } else {
                continue;
            }
        };
    }

    Ok(())
}
