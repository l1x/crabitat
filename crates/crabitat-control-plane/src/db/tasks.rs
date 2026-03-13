use crate::models::tasks::{CreateRunRequest, GitInfo, Run, Task, TaskWithGit};
use rusqlite::{Connection, params};

pub fn insert_task(
    conn: &Connection,
    mission_id: &str,
    step_id: &str,
    step_order: i64,
    assembled_prompt: &str,
    max_retries: i64,
    status: &str,
) -> Result<Task, String> {
    let task_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO tasks (task_id, mission_id, step_id, step_order, assembled_prompt, max_retries, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            task_id,
            mission_id,
            step_id,
            step_order,
            assembled_prompt,
            max_retries,
            status
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(Task {
        task_id,
        mission_id: mission_id.to_string(),
        step_id: step_id.to_string(),
        step_order,
        assembled_prompt: assembled_prompt.to_string(),
        status: status.to_string(),
        retry_count: 0,
        max_retries,
        created_at: "".to_string(),
        updated_at: None,
    })
}

pub fn list_tasks_for_mission(conn: &Connection, mission_id: &str) -> Result<Vec<Task>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT task_id, mission_id, step_id, step_order, assembled_prompt, status, retry_count, max_retries, created_at, updated_at
         FROM tasks WHERE mission_id = ?1 ORDER BY step_order ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([mission_id], |row| {
            Ok(Task {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                step_id: row.get(2)?,
                step_order: row.get(3)?,
                assembled_prompt: row.get(4)?,
                status: row.get(5)?,
                retry_count: row.get(6)?,
                max_retries: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut tasks = Vec::new();
    for task in rows {
        tasks.push(task.map_err(|e| e.to_string())?);
    }
    Ok(tasks)
}

pub fn get_next_queued_task(
    conn: &Connection,
    worker_id: Option<&str>,
) -> Result<Option<TaskWithGit>, String> {
    // Get oldest queued task along with Git info, prioritizing sticky worker if provided
    let mut stmt = conn.prepare(
        "SELECT t.task_id, t.mission_id, t.step_id, t.step_order, t.assembled_prompt, t.status, t.retry_count, t.max_retries, t.created_at, t.updated_at,
                r.repo_url, m.branch, r.local_path
         FROM tasks t
         JOIN missions m ON t.mission_id = m.mission_id
         JOIN repos r ON m.repo_id = r.repo_id
         WHERE t.status = 'queued'
           AND r.deleted_at IS NULL
         ORDER BY (CASE WHEN ?1 IS NOT NULL AND m.last_worker_id = ?1 THEN 1 ELSE 0 END) DESC, t.created_at ASC
         LIMIT 1"
    ).map_err(|e| e.to_string())?;

    let result = stmt.query_row(params![worker_id], |row| {
        Ok(TaskWithGit {
            task: Task {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                step_id: row.get(2)?,
                step_order: row.get(3)?,
                assembled_prompt: row.get(4)?,
                status: row.get(5)?,
                retry_count: row.get(6)?,
                max_retries: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            },
            git: GitInfo {
                repo_url: row.get(10)?,
                branch: row.get(11)?,
                local_path: row.get(12)?,
            },
        })
    });

    match result {
        Ok(res) => {
            // Stickiness is last-writer-wins: the most recent worker to pick up
            // a task from this mission gets affinity for subsequent tasks.
            if let Some(wid) = worker_id {
                conn.execute(
                    "UPDATE missions SET last_worker_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE mission_id = ?2",
                    params![wid, res.task.mission_id],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(Some(res))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn update_task_status(conn: &Connection, task_id: &str, status: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE task_id = ?2",
        params![status, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn increment_task_retry(conn: &Connection, task_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET status = 'queued', retry_count = retry_count + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn insert_run(conn: &Connection, task_id: &str, req: &CreateRunRequest) -> Result<Run, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO runs (run_id, task_id, status, logs, summary, duration_ms, tokens_used, finished_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        params![
            run_id,
            task_id,
            req.status,
            req.logs,
            req.summary,
            req.duration_ms,
            req.tokens_used
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(Run {
        run_id,
        task_id: task_id.to_string(),
        status: req.status.clone(),
        logs: req.logs.clone(),
        summary: req.summary.clone(),
        duration_ms: req.duration_ms,
        tokens_used: req.tokens_used,
        started_at: "".into(),
        finished_at: Some("".into()),
    })
}

pub fn list_runs_for_task(conn: &Connection, task_id: &str) -> Result<Vec<Run>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT run_id, task_id, status, logs, summary, duration_ms, tokens_used, started_at, finished_at 
         FROM runs WHERE task_id = ?1 ORDER BY started_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([task_id], |row| {
            Ok(Run {
                run_id: row.get(0)?,
                task_id: row.get(1)?,
                status: row.get(2)?,
                logs: row.get(3)?,
                summary: row.get(4)?,
                duration_ms: row.get(5)?,
                tokens_used: row.get(6)?,
                started_at: row.get(7)?,
                finished_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut runs = Vec::new();
    for run in rows {
        runs.push(run.map_err(|e| e.to_string())?);
    }
    Ok(runs)
}

pub fn get_task(conn: &Connection, task_id: &str) -> Result<Option<Task>, String> {
    let result = conn.query_row(
        "SELECT task_id, mission_id, step_id, step_order, assembled_prompt, status, retry_count, max_retries, created_at, updated_at
         FROM tasks WHERE task_id = ?1",
        [task_id],
        |row| {
            Ok(Task {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                step_id: row.get(2)?,
                step_order: row.get(3)?,
                assembled_prompt: row.get(4)?,
                status: row.get(5)?,
                retry_count: row.get(6)?,
                max_retries: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        },
    );

    match result {
        Ok(task) => Ok(Some(task)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn get_next_task_in_mission(
    conn: &Connection,
    mission_id: &str,
    after_step_order: i64,
) -> Result<Option<Task>, String> {
    let result = conn.query_row(
        "SELECT task_id, mission_id, step_id, step_order, assembled_prompt, status, retry_count, max_retries, created_at, updated_at
         FROM tasks WHERE mission_id = ?1 AND step_order > ?2
         ORDER BY step_order ASC LIMIT 1",
        params![mission_id, after_step_order],
        |row| {
            Ok(Task {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                step_id: row.get(2)?,
                step_order: row.get(3)?,
                assembled_prompt: row.get(4)?,
                status: row.get(5)?,
                retry_count: row.get(6)?,
                max_retries: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        },
    );

    match result {
        Ok(task) => Ok(Some(task)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn update_task_assembled_prompt(
    conn: &Connection,
    task_id: &str,
    assembled_prompt: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET assembled_prompt = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE task_id = ?2",
        params![assembled_prompt, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tasks_test;
