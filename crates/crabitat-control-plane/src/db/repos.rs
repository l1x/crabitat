use rusqlite::{Connection, params};

use crate::models::Repo;

pub fn insert(
    conn: &Connection,
    owner: &str,
    name: &str,
    local_path: Option<&str>,
    repo_url: Option<&str>,
) -> Result<Repo, String> {
    let repo_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO repos (repo_id, owner, name, local_path, repo_url) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![repo_id, owner, name, local_path, repo_url],
    )
    .map_err(|e| format!("repo already exists: {e}"))?;

    get_by_id(conn, &repo_id).map(|r| r.unwrap())
}

pub fn list(conn: &Connection) -> Result<Vec<Repo>, String> {
    let mut stmt = conn
        .prepare("SELECT repo_id, owner, name, local_path, created_at, repo_url, updated_at, deleted_at FROM repos WHERE deleted_at IS NULL ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let repos = stmt
        .query_map([], |row| {
            Ok(Repo {
                repo_id: row.get(0)?,
                owner: row.get(1)?,
                name: row.get(2)?,
                local_path: row.get(3)?,
                created_at: row.get(4)?,
                repo_url: row.get(5)?,
                updated_at: row.get(6)?,
                deleted_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(repos)
}

pub fn get_by_id(conn: &Connection, repo_id: &str) -> Result<Option<Repo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT repo_id, owner, name, local_path, created_at, repo_url, updated_at, deleted_at FROM repos WHERE repo_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let mut rows = stmt
        .query_map(params![repo_id], |row| {
            Ok(Repo {
                repo_id: row.get(0)?,
                owner: row.get(1)?,
                name: row.get(2)?,
                local_path: row.get(3)?,
                created_at: row.get(4)?,
                repo_url: row.get(5)?,
                updated_at: row.get(6)?,
                deleted_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;

    match rows.next() {
        Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

pub fn delete(conn: &Connection, repo_id: &str) -> Result<bool, String> {
    let affected = conn
        .execute(
            "UPDATE repos SET deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE repo_id = ?1 AND deleted_at IS NULL",
            params![repo_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(affected > 0)
}

pub fn update(
    conn: &Connection,
    repo_id: &str,
    local_path: Option<&str>,
    repo_url: Option<&str>,
) -> Result<bool, String> {
    let affected = conn
        .execute(
            "UPDATE repos SET local_path = ?1, repo_url = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE repo_id = ?3 AND deleted_at IS NULL",
            params![local_path, repo_url, repo_id],
        )
        .map_err(|e| e.to_string())? ;
    Ok(affected > 0)
}
