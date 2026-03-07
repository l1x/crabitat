use crate::models::missions::{CreateMissionRequest, Mission};
use rusqlite::{Connection, params};

pub fn insert_mission(conn: &Connection, req: &CreateMissionRequest) -> Result<Mission, String> {
    let mission_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO missions (mission_id, repo_id, issue_number, workflow_name, flavor_id) 
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            mission_id,
            req.repo_id,
            req.issue_number,
            req.workflow_name,
            req.flavor_id
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(Mission {
        mission_id,
        repo_id: req.repo_id.clone(),
        issue_number: req.issue_number,
        workflow_name: req.workflow_name.clone(),
        flavor_id: req.flavor_id.clone(),
        status: "pending".to_string(),
        created_at: "".to_string(), // Filled by query if needed
    })
}

pub fn get_mission(conn: &Connection, mission_id: &str) -> Result<Option<Mission>, String> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, repo_id, issue_number, workflow_name, flavor_id, status, created_at 
         FROM missions WHERE mission_id = ?1"
    ).map_err(|e| e.to_string())?;

    let mission = stmt.query_row([mission_id], |row| {
        Ok(Mission {
            mission_id: row.get(0)?,
            repo_id: row.get(1)?,
            issue_number: row.get(2)?,
            workflow_name: row.get(3)?,
            flavor_id: row.get(4)?,
            status: row.get(5)?,
            created_at: row.get(6)?,
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
        "SELECT mission_id, repo_id, issue_number, workflow_name, flavor_id, status, created_at 
         FROM missions ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Mission {
                mission_id: row.get(0)?,
                repo_id: row.get(1)?,
                issue_number: row.get(2)?,
                workflow_name: row.get(3)?,
                flavor_id: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut missions = Vec::new();
    for m in rows {
        missions.push(m.map_err(|e| e.to_string())?);
    }
    Ok(missions)
}
