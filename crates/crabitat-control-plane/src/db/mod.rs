pub mod issues;
pub mod repos;
pub mod workflows;

use rusqlite::Connection;

pub fn init(path: &str) -> Connection {
    let conn = Connection::open(path).expect("failed to open database");
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    migrate(&conn);
    conn
}

pub(crate) fn migrate(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS repos (
            repo_id    TEXT PRIMARY KEY,
            owner      TEXT NOT NULL,
            name       TEXT NOT NULL,
            local_path TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            UNIQUE(owner, name)
        );

        CREATE TABLE IF NOT EXISTS github_issues_cache (
            repo_id    TEXT NOT NULL REFERENCES repos(repo_id) ON DELETE CASCADE,
            number     INTEGER NOT NULL,
            title      TEXT NOT NULL,
            body       TEXT,
            labels     TEXT NOT NULL DEFAULT '[]',
            state      TEXT NOT NULL DEFAULT 'open',
            fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            PRIMARY KEY (repo_id, number)
        );

        CREATE TABLE IF NOT EXISTS workflows (
            workflow_id  TEXT PRIMARY KEY,
            repo_id      TEXT NOT NULL REFERENCES repos(repo_id) ON DELETE CASCADE,
            name         TEXT NOT NULL,
            description  TEXT NOT NULL DEFAULT '',
            created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            UNIQUE(repo_id, name)
        );

        CREATE TABLE IF NOT EXISTS workflow_steps (
            step_id         TEXT PRIMARY KEY,
            workflow_id     TEXT NOT NULL REFERENCES workflows(workflow_id) ON DELETE CASCADE,
            step_order      INTEGER NOT NULL,
            name            TEXT NOT NULL,
            prompt_template TEXT NOT NULL,
            UNIQUE(workflow_id, step_order)
        );

        CREATE TABLE IF NOT EXISTS workflow_flavors (
            flavor_id    TEXT PRIMARY KEY,
            workflow_id  TEXT NOT NULL REFERENCES workflows(workflow_id) ON DELETE CASCADE,
            name         TEXT NOT NULL,
            context      TEXT,
            UNIQUE(workflow_id, name)
        );",
    )
    .expect("failed to run migrations");
}
