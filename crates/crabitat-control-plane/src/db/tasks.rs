use crate::models::tasks::Task;
use rusqlite::{Connection, params};

pub fn insert_task(
    conn: &Connection,
    mission_id: &str,
    step_id: &str,
    step_order: i64,
    assembled_prompt: &str,
) -> Result<Task, String> {
    let task_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO tasks (task_id, mission_id, step_id, step_order, assembled_prompt) 
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, mission_id, step_id, step_order, assembled_prompt],
    )
    .map_err(|e| e.to_string())?;

    Ok(Task {
        task_id,
        mission_id: mission_id.to_string(),
        step_id: step_id.to_string(),
        step_order,
        assembled_prompt: assembled_prompt.to_string(),
        status: "queued".to_string(),
        created_at: "".to_string(),
    })
}

pub fn list_tasks_for_mission(conn: &Connection, mission_id: &str) -> Result<Vec<Task>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT task_id, mission_id, step_id, step_order, assembled_prompt, status, created_at 
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
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut tasks = Vec::new();
    for task in rows {
        tasks.push(task.map_err(|e| e.to_string())?);
    }
    Ok(tasks)
}

pub fn get_next_queued_task(conn: &Connection) -> Result<Option<(Task, String)>, String> {
    // Get oldest queued task along with the local_path of the repo it belongs to
    let mut stmt = conn.prepare(
        "SELECT t.task_id, t.mission_id, t.step_id, t.step_order, t.assembled_prompt, t.status, t.created_at, r.local_path
         FROM tasks t
         JOIN missions m ON t.mission_id = m.mission_id
         JOIN repos r ON m.repo_id = r.repo_id
         WHERE t.status = 'queued'
         ORDER BY t.created_at ASC
         LIMIT 1"
    ).map_err(|e| e.to_string())?;

    let result = stmt.query_row([], |row| {
        Ok((
            Task {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                step_id: row.get(2)?,
                step_order: row.get(3)?,
                assembled_prompt: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            },
            row.get(7)?,
        ))
    });

    match result {
        Ok(res) => Ok(Some(res)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn update_task_status(conn: &Connection, task_id: &str, status: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET status = ?1 WHERE task_id = ?2",
        params![status, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
