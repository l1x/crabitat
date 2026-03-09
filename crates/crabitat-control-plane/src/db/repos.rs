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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        db::migrate(&conn);
        conn
    }

    #[test]
    fn insert_and_list_repos() {
        let conn = setup();
        insert(&conn, "owner", "name", None, None).unwrap();
        let repos = list(&conn).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].owner, "owner");
        assert_eq!(repos[0].name, "name");
    }

    #[test]
    fn soft_delete_repo() {
        let conn = setup();
        let repo = insert(&conn, "owner", "name", None, None).unwrap();

        // Before delete: list should have 1
        assert_eq!(list(&conn).unwrap().len(), 1);

        // Delete it
        assert!(delete(&conn, &repo.repo_id).unwrap());

        // After delete: list should be empty
        assert_eq!(list(&conn).unwrap().len(), 0);

        // But get_by_id should still return it for history
        let deleted_repo = get_by_id(&conn, &repo.repo_id).unwrap().unwrap();
        assert!(deleted_repo.deleted_at.is_some());
    }

    #[test]
    fn delete_idempotency() {
        let conn = setup();
        let repo = insert(&conn, "owner", "name", None, None).unwrap();

        // First delete returns true
        assert!(delete(&conn, &repo.repo_id).unwrap());
        // Second delete returns false (already deleted)
        assert!(!delete(&conn, &repo.repo_id).unwrap());
    }

    #[test]
    fn get_nonexistent_repo() {
        let conn = setup();
        assert!(get_by_id(&conn, "no-such-id").unwrap().is_none());
    }

    #[test]
    fn re_add_soft_deleted_repo() {
        let conn = setup();
        let repo = insert(&conn, "owner", "name", None, None).unwrap();
        delete(&conn, &repo.repo_id).unwrap();

        // This should fail currently because of the UNIQUE(owner, name) constraint
        let result = insert(&conn, "owner", "name", None, None);
        assert!(
            result.is_ok(),
            "Should be able to re-add soft-deleted repo, but got: {:?}",
            result.err()
        );
    }

    #[test]
    fn active_duplicate_repo_rejected() {
        let conn = setup();
        insert(&conn, "owner", "name", None, None).unwrap();
        let result = insert(&conn, "owner", "name", None, None);
        assert!(
            result.is_err(),
            "Should NOT be able to add duplicate active repo"
        );
        let err = result.err().unwrap();
        assert!(err.contains("UNIQUE constraint failed"), "got: {err}");
    }
}
