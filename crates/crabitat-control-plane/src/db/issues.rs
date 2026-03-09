use rusqlite::{Connection, params};

use crate::models::Issue;

pub fn upsert_issues(conn: &Connection, repo_id: &str, issues: &[Issue]) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT OR REPLACE INTO github_issues_cache
                (repo_id, number, title, body, labels, state, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        )
        .map_err(|e| e.to_string())?;

    for issue in issues {
        let labels_json = serde_json::to_string(&issue.labels).unwrap_or_else(|_| "[]".into());
        stmt.execute(params![
            repo_id,
            issue.number,
            issue.title,
            issue.body,
            labels_json,
            issue.state,
        ])
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn list_by_repo(conn: &Connection, repo_id: &str) -> Result<Vec<Issue>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT repo_id, number, title, body, labels, state, fetched_at
             FROM github_issues_cache
             WHERE repo_id = ?1
             ORDER BY number DESC",
        )
        .map_err(|e| e.to_string())?;

    let issues = stmt
        .query_map(params![repo_id], |row| {
            let labels_str: String = row.get(4)?;
            let labels: Vec<String> = serde_json::from_str(&labels_str).unwrap_or_default();

            Ok(Issue {
                repo_id: row.get(0)?,
                number: row.get(1)?,
                title: row.get(2)?,
                body: row.get(3)?,
                labels,
                state: row.get(5)?,
                fetched_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(issues)
}

pub fn get_cached_issue(
    conn: &Connection,
    repo_id: &str,
    issue_number: i64,
) -> Result<Option<Issue>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT repo_id, number, title, body, labels, state, fetched_at
             FROM github_issues_cache
             WHERE repo_id = ?1 AND number = ?2",
        )
        .map_err(|e| e.to_string())?;

    let result = stmt.query_row(params![repo_id, issue_number], |row| {
        let labels_str: String = row.get(4)?;
        let labels: Vec<String> = serde_json::from_str(&labels_str).unwrap_or_default();
        Ok(Issue {
            repo_id: row.get(0)?,
            number: row.get(1)?,
            title: row.get(2)?,
            body: row.get(3)?,
            labels,
            state: row.get(5)?,
            fetched_at: row.get(6)?,
        })
    });

    match result {
        Ok(issue) => Ok(Some(issue)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn has_cached(conn: &Connection, repo_id: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM github_issues_cache WHERE repo_id = ?1")
        .map_err(|e| e.to_string())?;

    let count: i64 = stmt
        .query_row(params![repo_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    Ok(count > 0)
}
