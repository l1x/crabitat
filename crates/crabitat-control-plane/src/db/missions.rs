use crate::models::missions::{CreateMissionRequest, Mission};
use rusqlite::{Connection, params};

pub fn insert_mission(
    conn: &Connection,
    req: &CreateMissionRequest,
    branch: &str,
) -> Result<Mission, String> {
    let mission_id = uuid::Uuid::new_v4().to_string();

    // Fetch repo owner/name for hydration
    let (owner, name): (String, String) = conn
        .query_row(
            "SELECT owner, name FROM repos WHERE repo_id = ?1",
            [&req.repo_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO missions (mission_id, repo_id, issue_number, workflow_name, flavor_id, branch) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            mission_id,
            req.repo_id,
            req.issue_number,
            req.workflow_name,
            req.flavor_id,
            branch
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(Mission {
        mission_id,
        repo_id: req.repo_id.clone(),
        repo_owner: owner,
        repo_name: name,
        issue_number: req.issue_number,
        workflow_name: req.workflow_name.clone(),
        flavor_id: req.flavor_id.clone(),
        status: "pending".to_string(),
        created_at: "".to_string(),
        updated_at: None,
        branch: branch.to_string(),
        last_worker_id: None,
    })
}

pub fn get_mission(conn: &Connection, mission_id: &str) -> Result<Option<Mission>, String> {
    let mut stmt = conn.prepare(
        "SELECT m.mission_id, m.repo_id, r.owner, r.name, m.issue_number, m.workflow_name, m.flavor_id, m.status, m.created_at, m.updated_at, m.branch, m.last_worker_id
         FROM missions m
         JOIN repos r ON m.repo_id = r.repo_id
         WHERE m.mission_id = ?1"
    ).map_err(|e| e.to_string())?;

    let mission = stmt.query_row([mission_id], |row| {
        Ok(Mission {
            mission_id: row.get(0)?,
            repo_id: row.get(1)?,
            repo_owner: row.get(2)?,
            repo_name: row.get(3)?,
            issue_number: row.get(4)?,
            workflow_name: row.get(5)?,
            flavor_id: row.get(6)?,
            status: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            branch: row.get(10)?,
            last_worker_id: row.get(11)?,
        })
    });

    match mission {
        Ok(m) => Ok(Some(m)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn list_all(conn: &Connection) -> Result<Vec<Mission>, String> {
    let mut stmt = conn.prepare(
        "SELECT m.mission_id, m.repo_id, r.owner, r.name, m.issue_number, m.workflow_name, m.flavor_id, m.status, m.created_at, m.updated_at, m.branch, m.last_worker_id
         FROM missions m
         JOIN repos r ON m.repo_id = r.repo_id
         ORDER BY m.created_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Mission {
                mission_id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_owner: row.get(2)?,
                repo_name: row.get(3)?,
                issue_number: row.get(4)?,
                workflow_name: row.get(5)?,
                flavor_id: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                branch: row.get(10)?,
                last_worker_id: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut missions = Vec::new();
    for m in rows {
        missions.push(m.map_err(|e| e.to_string())?);
    }
    Ok(missions)
}

pub fn list_by_repo(conn: &Connection, repo_id: &str) -> Result<Vec<Mission>, String> {
    let mut stmt = conn.prepare(
        "SELECT m.mission_id, m.repo_id, r.owner, r.name, m.issue_number, m.workflow_name, m.flavor_id, m.status, m.created_at, m.updated_at, m.branch, m.last_worker_id
         FROM missions m
         JOIN repos r ON m.repo_id = r.repo_id
         WHERE m.repo_id = ?1
         ORDER BY m.created_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([repo_id], |row| {
            Ok(Mission {
                mission_id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_owner: row.get(2)?,
                repo_name: row.get(3)?,
                issue_number: row.get(4)?,
                workflow_name: row.get(5)?,
                flavor_id: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                branch: row.get(10)?,
                last_worker_id: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut missions = Vec::new();
    for m in rows {
        missions.push(m.map_err(|e| e.to_string())?);
    }
    Ok(missions)
}

pub fn recalculate_mission_status(conn: &Connection, mission_id: &str) -> Result<(), String> {
    // Get all task statuses for this mission
    let mut stmt = conn
        .prepare("SELECT status FROM tasks WHERE mission_id = ?1")
        .map_err(|e| e.to_string())?;

    let statuses: Vec<String> = stmt
        .query_map([mission_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if statuses.is_empty() {
        return Ok(());
    }

    let new_status = if statuses.iter().any(|s| s == "failed") {
        "failed"
    } else if statuses.iter().all(|s| s == "completed") {
        "completed"
    } else if statuses.iter().any(|s| s == "running") {
        "running"
    } else {
        "pending"
    };

    conn.execute(
        "UPDATE missions SET status = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE mission_id = ?2",
        params![new_status, mission_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod missions_test;
