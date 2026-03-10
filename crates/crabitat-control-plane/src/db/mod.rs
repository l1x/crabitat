pub mod issues;
pub mod missions;
pub mod repos;
pub mod settings;
pub mod tasks;
pub mod workflows;

use rusqlite::{Connection, params};

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
            local_path TEXT,
            repo_url   TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at TEXT,
            deleted_at TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS repos_owner_name_uniq
            ON repos(owner, name) WHERE deleted_at IS NULL;

        CREATE TABLE IF NOT EXISTS github_issues_cache (
            repo_id    TEXT NOT NULL REFERENCES repos(repo_id),
            number     INTEGER NOT NULL,
            title      TEXT NOT NULL,
            body       TEXT,
            labels     TEXT NOT NULL DEFAULT '[]',
            state      TEXT NOT NULL DEFAULT 'open',
            fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            PRIMARY KEY (repo_id, number)
        );

        CREATE TABLE IF NOT EXISTS workflow_flavors (
            flavor_id     TEXT PRIMARY KEY,
            workflow_name TEXT NOT NULL,
            name          TEXT NOT NULL,
            prompt_paths  TEXT NOT NULL DEFAULT '[]',
            created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at    TEXT,
            deleted_at    TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS workflow_flavors_name_uniq
            ON workflow_flavors(workflow_name, name) WHERE deleted_at IS NULL;

        CREATE TABLE IF NOT EXISTS settings (
            key        TEXT PRIMARY KEY,
            value      TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at TEXT
        );

        CREATE TABLE IF NOT EXISTS environment_paths (
            environment   TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_name TEXT NOT NULL,
            path          TEXT NOT NULL,
            created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at    TEXT,
            PRIMARY KEY (environment, resource_type, resource_name)
        );

        -- Execution Layer (FR-4)

        CREATE TABLE IF NOT EXISTS missions (
            mission_id    TEXT PRIMARY KEY,
            repo_id       TEXT NOT NULL REFERENCES repos(repo_id),
            issue_number  INTEGER NOT NULL,
            workflow_name TEXT NOT NULL,
            flavor_id     TEXT REFERENCES workflow_flavors(flavor_id),
            status        TEXT NOT NULL DEFAULT 'pending',
            branch        TEXT,
            created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at    TEXT,
            repo_owner    TEXT,
            repo_name     TEXT,
            last_worker_id TEXT,
            FOREIGN KEY (repo_id, issue_number) REFERENCES github_issues_cache(repo_id, number)
        );

        CREATE TABLE IF NOT EXISTS tasks (
            task_id          TEXT PRIMARY KEY,
            mission_id       TEXT NOT NULL REFERENCES missions(mission_id),
            step_id          TEXT NOT NULL,
            step_order       INTEGER NOT NULL,
            assembled_prompt TEXT NOT NULL,
            status           TEXT NOT NULL DEFAULT 'queued',
            retry_count      INTEGER DEFAULT 0,
            max_retries      INTEGER DEFAULT 3,
            created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at       TEXT
        );

        CREATE TABLE IF NOT EXISTS runs (
            run_id      TEXT PRIMARY KEY,
            task_id     TEXT NOT NULL REFERENCES tasks(task_id),
            status      TEXT NOT NULL,
            logs        TEXT,
            summary     TEXT,
            duration_ms INTEGER,
            tokens_used INTEGER,
            started_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            finished_at TEXT
        );",
    )
    .expect("failed to run migrations");

    // Add columns for existing databases (ALTER TABLE cannot use non-constant DEFAULT)
    for stmt in &[
        "ALTER TABLE repos ADD COLUMN deleted_at TEXT",
        "ALTER TABLE repos ADD COLUMN updated_at TEXT",
        "ALTER TABLE workflow_flavors ADD COLUMN deleted_at TEXT",
        "ALTER TABLE workflow_flavors ADD COLUMN created_at TEXT",
        "ALTER TABLE workflow_flavors ADD COLUMN updated_at TEXT",
        "ALTER TABLE settings ADD COLUMN created_at TEXT",
        "ALTER TABLE settings ADD COLUMN updated_at TEXT",
        "ALTER TABLE environment_paths ADD COLUMN created_at TEXT",
        "ALTER TABLE environment_paths ADD COLUMN updated_at TEXT",
        "ALTER TABLE missions ADD COLUMN updated_at TEXT",
        "ALTER TABLE missions ADD COLUMN last_worker_id TEXT",
        "ALTER TABLE tasks ADD COLUMN updated_at TEXT",
    ] {
        match conn.execute(stmt, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => panic!("migration failed: {e}"),
        }
    }

    // Backfill created_at for rows added before the column existed
    for stmt in &[
        "UPDATE workflow_flavors SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE created_at IS NULL",
        "UPDATE settings SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE created_at IS NULL",
        "UPDATE environment_paths SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE created_at IS NULL",
    ] {
        conn.execute(stmt, [])
            .expect("failed to backfill created_at");
    }

    // Migration: Remove UNIQUE constraints from repos and workflow_flavors by rebuilding tables
    // This is necessary because SQLite doesn't support DROP CONSTRAINT.
    for table in &["repos", "workflow_flavors"] {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |row| row.get(0),
            )
            .unwrap_or_default();

        let needs_rebuild = if *table == "repos" {
            sql.contains("UNIQUE") && sql.contains("owner") && sql.contains("name")
        } else {
            sql.contains("UNIQUE") && sql.contains("workflow_name") && sql.contains("name")
        };

        if needs_rebuild {
            let (create_sql, columns, index_name, index_cols) = if *table == "repos" {
                (
                    "CREATE TABLE repos_new (
                        repo_id    TEXT PRIMARY KEY,
                        owner      TEXT NOT NULL,
                        name       TEXT NOT NULL,
                        local_path TEXT,
                        repo_url   TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                        updated_at TEXT,
                        deleted_at TEXT
                    )",
                    "repo_id, owner, name, local_path, repo_url, created_at, updated_at, deleted_at",
                    "repos_owner_name_uniq",
                    "owner, name",
                )
            } else {
                (
                    "CREATE TABLE workflow_flavors_new (
                        flavor_id     TEXT PRIMARY KEY,
                        workflow_name TEXT NOT NULL,
                        name          TEXT NOT NULL,
                        prompt_paths  TEXT NOT NULL DEFAULT '[]',
                        created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                        updated_at    TEXT,
                        deleted_at    TEXT
                    )",
                    "flavor_id, workflow_name, name, prompt_paths, created_at, updated_at, deleted_at",
                    "workflow_flavors_name_uniq",
                    "workflow_name, name",
                )
            };

            conn.execute_batch(&format!(
                "PRAGMA foreign_keys = OFF;
                 BEGIN TRANSACTION;
                 {create_sql};
                 INSERT INTO {table}_new ({columns}) SELECT {columns} FROM {table};
                 DROP TABLE {table};
                 ALTER TABLE {table}_new RENAME TO {table};
                 CREATE UNIQUE INDEX IF NOT EXISTS {index_name} ON {table}({index_cols}) WHERE deleted_at IS NULL;
                 COMMIT;
                 PRAGMA foreign_key_check;
                 PRAGMA foreign_keys = ON;"
            ))
            .expect("failed to rebuild table");
        }
    }
}
