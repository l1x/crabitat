use crate::error::ApiError;
use crate::github::GhIssue;
use crate::types::*;
use anyhow::Result;
use crabitat_core::{BurrowMode, MissionStatus, RunMetrics, RunStatus, TaskStatus, now_ms};
use rusqlite::{Connection, params};
use std::path::Path as StdPath;
use tracing::info;

// ---------------------------------------------------------------------------
// Schema & migrations
// ---------------------------------------------------------------------------

pub(crate) fn apply_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS repos (
          repo_id TEXT PRIMARY KEY,
          owner TEXT NOT NULL,
          name TEXT NOT NULL,
          default_branch TEXT NOT NULL DEFAULT 'main',
          language TEXT NOT NULL DEFAULT '',
          local_path TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          UNIQUE(owner, name)
        );

        CREATE TABLE IF NOT EXISTS crabs (
          crab_id TEXT PRIMARY KEY,
          repo_id TEXT NOT NULL,
          name TEXT NOT NULL,
          state TEXT NOT NULL,
          current_task_id TEXT,
          current_run_id TEXT,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(repo_id) REFERENCES repos(repo_id)
        );

        CREATE TABLE IF NOT EXISTS missions (
          mission_id TEXT PRIMARY KEY,
          repo_id TEXT NOT NULL,
          prompt TEXT NOT NULL,
          workflow_name TEXT,
          status TEXT NOT NULL DEFAULT 'pending',
          worktree_path TEXT,
          queue_position INTEGER,
          github_issue_number INTEGER,
          github_pr_number INTEGER,
          created_at_ms INTEGER NOT NULL,
          FOREIGN KEY(repo_id) REFERENCES repos(repo_id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
          task_id TEXT PRIMARY KEY,
          mission_id TEXT NOT NULL,
          title TEXT NOT NULL,
          assigned_crab_id TEXT,
          status TEXT NOT NULL,
          step_id TEXT,
          prompt TEXT,
          context TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(mission_id) REFERENCES missions(mission_id)
        );

        CREATE TABLE IF NOT EXISTS task_deps (
          task_id TEXT NOT NULL,
          depends_on_task_id TEXT NOT NULL,
          PRIMARY KEY (task_id, depends_on_task_id)
        );

        CREATE TABLE IF NOT EXISTS runs (
          run_id TEXT PRIMARY KEY,
          mission_id TEXT NOT NULL,
          task_id TEXT NOT NULL,
          crab_id TEXT NOT NULL,
          status TEXT NOT NULL,
          burrow_path TEXT NOT NULL,
          burrow_mode TEXT NOT NULL,
          progress_message TEXT NOT NULL,
          summary TEXT,
          prompt_tokens INTEGER NOT NULL DEFAULT 0,
          completion_tokens INTEGER NOT NULL DEFAULT 0,
          total_tokens INTEGER NOT NULL DEFAULT 0,
          first_token_ms INTEGER,
          llm_duration_ms INTEGER,
          execution_duration_ms INTEGER,
          end_to_end_ms INTEGER,
          started_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          completed_at_ms INTEGER,
          FOREIGN KEY(mission_id) REFERENCES missions(mission_id),
          FOREIGN KEY(task_id) REFERENCES tasks(task_id)
        );

        CREATE TABLE IF NOT EXISTS workflows (
          workflow_id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          description TEXT NOT NULL DEFAULT '',
          include TEXT NOT NULL DEFAULT '[]',
          version TEXT NOT NULL DEFAULT '1.0.0',
          created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workflow_steps (
          workflow_id TEXT NOT NULL,
          position INTEGER NOT NULL,
          step_id TEXT NOT NULL,
          prompt_file TEXT NOT NULL DEFAULT '',
          depends_on TEXT NOT NULL DEFAULT '[]',
          condition TEXT,
          max_retries INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY (workflow_id, position),
          FOREIGN KEY(workflow_id) REFERENCES workflows(workflow_id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS github_issues_cache (
          repo_id       TEXT    NOT NULL,
          number        INTEGER NOT NULL,
          title         TEXT    NOT NULL,
          body          TEXT    NOT NULL DEFAULT '',
          labels        TEXT    NOT NULL DEFAULT '[]',
          state         TEXT    NOT NULL DEFAULT 'OPEN',
          fetched_at_ms INTEGER NOT NULL,
          PRIMARY KEY (repo_id, number),
          FOREIGN KEY(repo_id) REFERENCES repos(repo_id) ON DELETE CASCADE
        );
        ",
    )?;

    // Migrations: add columns to existing tables (safe to re-run)
    let migrations = [
        "ALTER TABLE missions ADD COLUMN workflow_name TEXT",
        "ALTER TABLE missions ADD COLUMN queue_position INTEGER",
        "ALTER TABLE missions ADD COLUMN github_issue_number INTEGER",
        "ALTER TABLE missions ADD COLUMN github_pr_number INTEGER",
        "ALTER TABLE repos ADD COLUMN language TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE workflow_steps ADD COLUMN include TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE workflows RENAME COLUMN stack TO include",
        "ALTER TABLE workflow_steps RENAME COLUMN stack TO include",
        "ALTER TABLE workflows ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'",
        "ALTER TABLE workflows ADD COLUMN commit_hash TEXT",
        "ALTER TABLE missions RENAME COLUMN colony_id TO repo_id",
        "ALTER TABLE crabs RENAME COLUMN colony_id TO repo_id",
        "ALTER TABLE repos ADD COLUMN stacks TEXT NOT NULL DEFAULT '[]'",
    ];
    for sql in migrations {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("duplicate column")
                    || e.to_string().contains("no such column") =>
            {
            }
            Err(e) => return Err(e),
        }
    }

    // Drop roles table (removed feature)
    let _ = conn.execute("DROP TABLE IF EXISTS roles", []);

    // Migrate workflow include from plain string to JSON array (idempotent)
    let _ = conn.execute(
        "UPDATE workflows SET include = '[\"' || include || '\"]' WHERE include NOT LIKE '[%'",
        [],
    );

    // Drop-column migrations (safe to re-run)
    let drop_migrations = [
        "ALTER TABLE repos DROP COLUMN domain",
        "ALTER TABLE crabs DROP COLUMN role",
        "ALTER TABLE tasks DROP COLUMN role",
        "ALTER TABLE workflow_steps DROP COLUMN role",
    ];
    for sql in drop_migrations {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("no such column") => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub(crate) fn init_db(db_path: &StdPath) -> Result<Connection> {
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    apply_schema(&conn)?;
    Ok(conn)
}

pub(crate) fn seed_default_workflows(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }
    info!("no default workflows to seed — onboard from a prompts repo");
    Ok(())
}

pub(crate) fn seed_settings(conn: &Connection, prompts_path: &StdPath) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('prompts_path', ?1)",
        params![prompts_path.display().to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_stacks_json(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

pub(crate) fn query_repos(conn: &Connection) -> Result<Vec<RepoRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT repo_id, owner, name, default_branch, language, local_path, stacks, created_at_ms FROM repos ORDER BY created_at_ms DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let owner: String = row.get(1)?;
        let name: String = row.get(2)?;
        let full_name = format!("{owner}/{name}");
        let stacks_json: String = row.get(6)?;
        Ok(RepoRecord {
            repo_id: row.get(0)?,
            owner,
            name,
            full_name,
            default_branch: row.get(3)?,
            language: row.get(4)?,
            local_path: row.get(5)?,
            stacks: parse_stacks_json(&stacks_json),
            created_at_ms: row.get::<_, i64>(7)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn fetch_repo(conn: &Connection, repo_id: &str) -> Result<Option<RepoRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT repo_id, owner, name, default_branch, language, local_path, stacks, created_at_ms FROM repos WHERE repo_id = ?1",
    )?;
    let mut rows = stmt.query(params![repo_id])?;
    if let Some(row) = rows.next()? {
        let owner: String = row.get(1)?;
        let name: String = row.get(2)?;
        let full_name = format!("{owner}/{name}");
        let stacks_json: String = row.get(6)?;
        return Ok(Some(RepoRecord {
            repo_id: row.get(0)?,
            owner,
            name,
            full_name,
            default_branch: row.get(3)?,
            language: row.get(4)?,
            local_path: row.get(5)?,
            stacks: parse_stacks_json(&stacks_json),
            created_at_ms: row.get::<_, i64>(7)? as u64,
        }));
    }
    Ok(None)
}

pub(crate) fn query_crabs(conn: &Connection) -> Result<Vec<CrabRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT crab_id, repo_id, name, state, current_task_id, current_run_id, updated_at_ms FROM crabs ORDER BY crab_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CrabRecord {
            crab_id: row.get(0)?,
            repo_id: row.get(1)?,
            name: row.get(2)?,
            state: CrabState::from_str(&row.get::<_, String>(3)?),
            current_task_id: row.get(4)?,
            current_run_id: row.get(5)?,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn query_missions(conn: &Connection) -> Result<Vec<MissionRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, repo_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions ORDER BY created_at_ms DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MissionRecord {
            mission_id: row.get(0)?,
            repo_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn fetch_mission(conn: &Connection, mission_id: &str) -> Result<Option<MissionRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, repo_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions WHERE mission_id = ?1",
    )?;
    let mut rows = stmt.query(params![mission_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(MissionRecord {
            mission_id: row.get(0)?,
            repo_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        }));
    }
    Ok(None)
}

pub(crate) fn query_tasks(conn: &Connection) -> Result<Vec<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status,
               step_id, prompt, context,
               created_at_ms, updated_at_ms
        FROM tasks
        ORDER BY updated_at_ms DESC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TaskRecord {
            task_id: row.get(0)?,
            mission_id: row.get(1)?,
            title: row.get(2)?,
            assigned_crab_id: row.get(3)?,
            status: task_status_from_db(&row.get::<_, String>(4)?),
            step_id: row.get(5)?,
            prompt: row.get(6)?,
            context: row.get(7)?,
            created_at_ms: row.get::<_, i64>(8)? as u64,
            updated_at_ms: row.get::<_, i64>(9)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn query_runs(conn: &Connection) -> Result<Vec<RunRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
               progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
               first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
               started_at_ms, updated_at_ms, completed_at_ms
        FROM runs
        ORDER BY updated_at_ms DESC
        ",
    )?;
    let rows = stmt.query_map([], map_run_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn fetch_crab(conn: &Connection, crab_id: &str) -> Result<Option<CrabRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT crab_id, repo_id, name, state, current_task_id, current_run_id, updated_at_ms
        FROM crabs WHERE crab_id = ?1
        ",
    )?;

    let mut rows = stmt.query(params![crab_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(CrabRecord {
            crab_id: row.get(0)?,
            repo_id: row.get(1)?,
            name: row.get(2)?,
            state: CrabState::from_str(&row.get::<_, String>(3)?),
            current_task_id: row.get(4)?,
            current_run_id: row.get(5)?,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
        }));
    }
    Ok(None)
}

pub(crate) fn fetch_task(conn: &Connection, task_id: &str) -> Result<Option<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status,
               step_id, prompt, context,
               created_at_ms, updated_at_ms
        FROM tasks WHERE task_id = ?1
        ",
    )?;

    let mut rows = stmt.query(params![task_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(TaskRecord {
            task_id: row.get(0)?,
            mission_id: row.get(1)?,
            title: row.get(2)?,
            assigned_crab_id: row.get(3)?,
            status: task_status_from_db(&row.get::<_, String>(4)?),
            step_id: row.get(5)?,
            prompt: row.get(6)?,
            context: row.get(7)?,
            created_at_ms: row.get::<_, i64>(8)? as u64,
            updated_at_ms: row.get::<_, i64>(9)? as u64,
        }));
    }
    Ok(None)
}

pub(crate) fn fetch_run(conn: &Connection, run_id: &str) -> Result<Option<RunRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
               progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
               first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
               started_at_ms, updated_at_ms, completed_at_ms
        FROM runs
        WHERE run_id = ?1
        ",
    )?;
    let mut rows = stmt.query(params![run_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(map_run_row(row)?));
    }
    Ok(None)
}

pub(crate) fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        run_id: row.get(0)?,
        mission_id: row.get(1)?,
        task_id: row.get(2)?,
        crab_id: row.get(3)?,
        status: run_status_from_db(&row.get::<_, String>(4)?),
        burrow_path: row.get(5)?,
        burrow_mode: burrow_mode_from_db(&row.get::<_, String>(6)?),
        progress_message: row.get(7)?,
        summary: row.get(8)?,
        metrics: RunMetrics {
            prompt_tokens: row.get::<_, i64>(9)? as u32,
            completion_tokens: row.get::<_, i64>(10)? as u32,
            total_tokens: row.get::<_, i64>(11)? as u32,
            first_token_ms: row.get::<_, Option<i64>>(12)?.map(|v| v as u64),
            llm_duration_ms: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
            execution_duration_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
            end_to_end_ms: row.get::<_, Option<i64>>(15)?.map(|v| v as u64),
        },
        started_at_ms: row.get::<_, i64>(16)? as u64,
        updated_at_ms: row.get::<_, i64>(17)? as u64,
        completed_at_ms: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
    })
}

// ---------------------------------------------------------------------------
// Workflow query helpers
// ---------------------------------------------------------------------------

pub(crate) fn query_workflows(conn: &Connection) -> Result<Vec<WorkflowRecord>, ApiError> {
    let mut wf_stmt = conn.prepare(
        "SELECT workflow_id, name, description, include, version, created_at_ms, source, commit_hash FROM workflows ORDER BY name",
    )?;
    let workflows: Vec<(String, String, String, String, String, i64, String, Option<String>)> = wf_stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?))
        })?
        .filter_map(Result::ok)
        .collect();

    let mut result = Vec::with_capacity(workflows.len());
    for (wf_id, name, description, include_raw, version, created_at_ms, source, commit_hash) in workflows {
        let include: Vec<String> = serde_json::from_str(&include_raw).unwrap_or_default();
        let steps = query_workflow_steps(conn, &wf_id)?;
        result.push(WorkflowRecord {
            workflow_id: wf_id,
            name,
            description,
            include,
            version,
            source,
            commit_hash,
            created_at_ms: created_at_ms as u64,
            steps,
        });
    }
    Ok(result)
}

pub(crate) fn fetch_workflow(
    conn: &Connection,
    workflow_id: &str,
) -> Result<Option<WorkflowRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT workflow_id, name, description, include, version, created_at_ms, source, commit_hash FROM workflows WHERE workflow_id = ?1",
    )?;
    let mut rows = stmt.query(params![workflow_id])?;
    if let Some(row) = rows.next()? {
        let wf_id: String = row.get(0)?;
        let include_raw: String = row.get(3)?;
        let include: Vec<String> = serde_json::from_str(&include_raw).unwrap_or_default();
        let steps = query_workflow_steps(conn, &wf_id)?;
        return Ok(Some(WorkflowRecord {
            workflow_id: wf_id,
            name: row.get(1)?,
            description: row.get(2)?,
            include,
            version: row.get(4)?,
            source: row.get(6)?,
            commit_hash: row.get(7)?,
            created_at_ms: row.get::<_, i64>(5)? as u64,
            steps,
        }));
    }
    Ok(None)
}

pub(crate) fn query_workflow_steps(
    conn: &Connection,
    workflow_id: &str,
) -> Result<Vec<WorkflowStepRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT step_id, prompt_file, depends_on, condition, max_retries, position, include FROM workflow_steps WHERE workflow_id = ?1 ORDER BY position",
    )?;
    let rows = stmt.query_map(params![workflow_id], |row| {
        let depends_on_raw: String = row.get(2)?;
        let depends_on: Vec<String> = serde_json::from_str(&depends_on_raw).unwrap_or_default();
        let include_raw: String = row.get(6)?;
        let include: Vec<String> = serde_json::from_str(&include_raw).unwrap_or_default();
        Ok(WorkflowStepRecord {
            step_id: row.get(0)?,
            prompt_file: row.get(1)?,
            depends_on,
            condition: row.get(3)?,
            max_retries: row.get::<_, i64>(4)? as u32,
            position: row.get(5)?,
            include,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub(crate) fn insert_workflow_steps(
    conn: &Connection,
    workflow_id: &str,
    steps: &[CreateWorkflowStepInput],
) -> Result<(), ApiError> {
    for (i, step) in steps.iter().enumerate() {
        let depends_on_json = serde_json::to_string(&step.depends_on.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());
        let include_json = serde_json::to_string(&step.include.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT INTO workflow_steps (workflow_id, position, step_id, prompt_file, depends_on, condition, max_retries, include) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                workflow_id,
                i as i64,
                step.step_id,
                step.prompt_file.as_deref().unwrap_or(""),
                depends_on_json,
                step.condition,
                step.max_retries.unwrap_or(0) as i64,
                include_json,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn query_all_repo_stacks(conn: &Connection) -> Result<Vec<Vec<String>>, ApiError> {
    let mut stmt = conn.prepare("SELECT stacks FROM repos")?;
    let rows = stmt.query_map([], |row| {
        let json: String = row.get(0)?;
        Ok(parse_stacks_json(&json))
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

// ---------------------------------------------------------------------------
// Issue cache helpers
// ---------------------------------------------------------------------------

pub(crate) fn read_cached_issues(conn: &Connection, repo_id: &str) -> Option<(Vec<GhIssue>, u64)> {
    let mut stmt = conn
        .prepare(
            "SELECT number, title, body, labels, state, fetched_at_ms \
             FROM github_issues_cache WHERE repo_id = ?1 ORDER BY number",
        )
        .ok()?;

    let rows: Vec<(GhIssue, u64)> = stmt
        .query_map(params![repo_id], |row| {
            let labels_json: String = row.get(3)?;
            let labels: Vec<String> =
                serde_json::from_str(&labels_json).unwrap_or_default();
            Ok((
                GhIssue {
                    number: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    labels,
                    state: row.get(4)?,
                },
                row.get::<_, u64>(5)?,
            ))
        })
        .ok()?
        .filter_map(Result::ok)
        .collect();

    if rows.is_empty() {
        return None;
    }

    let fetched_at_ms = rows.iter().map(|(_, ts)| *ts).min().unwrap_or(0);
    let issues = rows.into_iter().map(|(issue, _)| issue).collect();
    Some((issues, fetched_at_ms))
}

pub(crate) fn write_issues_cache(
    conn: &Connection,
    repo_id: &str,
    issues: &[GhIssue],
) -> Result<(), ApiError> {
    let now = now_ms();
    conn.execute(
        "DELETE FROM github_issues_cache WHERE repo_id = ?1",
        params![repo_id],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO github_issues_cache (repo_id, number, title, body, labels, state, fetched_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for issue in issues {
        let labels_json = serde_json::to_string(&issue.labels).unwrap_or_else(|_| "[]".to_string());
        stmt.execute(params![
            repo_id,
            issue.number,
            issue.title,
            issue.body,
            labels_json,
            issue.state,
            now
        ])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Status conversion helpers
// ---------------------------------------------------------------------------

pub(crate) fn task_status_to_db(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Assigned => "assigned",
        TaskStatus::Running => "running",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
    }
}

pub(crate) fn task_status_from_db(raw: &str) -> TaskStatus {
    match raw {
        "assigned" => TaskStatus::Assigned,
        "running" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        "skipped" => TaskStatus::Skipped,
        _ => TaskStatus::Queued,
    }
}

pub(crate) fn mission_status_to_db(status: MissionStatus) -> &'static str {
    match status {
        MissionStatus::Pending => "pending",
        MissionStatus::Running => "running",
        MissionStatus::Completed => "completed",
        MissionStatus::Failed => "failed",
    }
}

pub(crate) fn mission_status_from_db(raw: &str) -> MissionStatus {
    match raw {
        "running" => MissionStatus::Running,
        "completed" => MissionStatus::Completed,
        "failed" => MissionStatus::Failed,
        _ => MissionStatus::Pending,
    }
}

pub(crate) fn run_status_to_db(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Blocked => "blocked",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
    }
}

pub(crate) fn run_status_from_db(raw: &str) -> RunStatus {
    match raw {
        "running" => RunStatus::Running,
        "blocked" => RunStatus::Blocked,
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        _ => RunStatus::Queued,
    }
}

pub(crate) fn burrow_mode_to_db(mode: BurrowMode) -> &'static str {
    match mode {
        BurrowMode::Worktree => "worktree",
        BurrowMode::ExternalRepo => "external_repo",
    }
}

pub(crate) fn burrow_mode_from_db(raw: &str) -> BurrowMode {
    match raw {
        "external_repo" => BurrowMode::ExternalRepo,
        _ => BurrowMode::Worktree,
    }
}

// ---------------------------------------------------------------------------
// Metric merge utility
// ---------------------------------------------------------------------------

pub(crate) fn merge_metrics(
    base: RunMetrics,
    usage_patch: Option<TokenUsagePatch>,
    timing_patch: Option<TimingPatch>,
) -> RunMetrics {
    let mut merged = base;
    if let Some(usage) = usage_patch {
        if let Some(v) = usage.prompt_tokens {
            merged.prompt_tokens = v;
        }
        if let Some(v) = usage.completion_tokens {
            merged.completion_tokens = v;
        }
        merged.total_tokens = usage
            .total_tokens
            .unwrap_or_else(|| merged.prompt_tokens.saturating_add(merged.completion_tokens));
    }
    if let Some(timing) = timing_patch {
        if timing.first_token_ms.is_some() {
            merged.first_token_ms = timing.first_token_ms;
        }
        if timing.llm_duration_ms.is_some() {
            merged.llm_duration_ms = timing.llm_duration_ms;
        }
        if timing.execution_duration_ms.is_some() {
            merged.execution_duration_ms = timing.execution_duration_ms;
        }
        if timing.end_to_end_ms.is_some() {
            merged.end_to_end_ms = timing.end_to_end_ms;
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::GhIssue;
    use crabitat_core::{BurrowMode, MissionStatus, RunMetrics, RunStatus, TaskStatus};
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn apply_schema_succeeds() {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
    }

    #[test]
    fn apply_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
        apply_schema(&conn).unwrap();
    }

    #[test]
    fn parse_stacks_json_valid() {
        let result = parse_stacks_json(r#"["rust","ts"]"#);
        assert_eq!(result, vec!["rust", "ts"]);
    }

    #[test]
    fn parse_stacks_json_empty() {
        let result = parse_stacks_json("[]");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_stacks_json_invalid() {
        let result = parse_stacks_json("garbage");
        assert!(result.is_empty());
    }

    #[test]
    fn task_status_roundtrip() {
        let variants = [
            TaskStatus::Queued,
            TaskStatus::Assigned,
            TaskStatus::Running,
            TaskStatus::Blocked,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Skipped,
        ];
        for status in variants {
            let db = task_status_to_db(status);
            let back = task_status_from_db(db);
            assert_eq!(back, status);
        }
    }

    #[test]
    fn mission_status_roundtrip() {
        let variants = [
            MissionStatus::Pending,
            MissionStatus::Running,
            MissionStatus::Completed,
            MissionStatus::Failed,
        ];
        for status in variants {
            let db = mission_status_to_db(status);
            let back = mission_status_from_db(db);
            assert_eq!(back, status);
        }
    }

    #[test]
    fn run_status_roundtrip() {
        let variants = [
            RunStatus::Queued,
            RunStatus::Running,
            RunStatus::Blocked,
            RunStatus::Completed,
            RunStatus::Failed,
        ];
        for status in variants {
            let db = run_status_to_db(status);
            let back = run_status_from_db(db);
            assert_eq!(back, status);
        }
    }

    #[test]
    fn burrow_mode_roundtrip() {
        let variants = [BurrowMode::Worktree, BurrowMode::ExternalRepo];
        for mode in variants {
            let db = burrow_mode_to_db(mode);
            let back = burrow_mode_from_db(db);
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn merge_metrics_usage_only() {
        let base = RunMetrics::default();
        let usage = TokenUsagePatch {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: None,
        };
        let result = merge_metrics(base, Some(usage), None);
        assert_eq!(result.prompt_tokens, 100);
        assert_eq!(result.completion_tokens, 50);
        assert_eq!(result.total_tokens, 150);
        assert!(result.first_token_ms.is_none());
    }

    #[test]
    fn merge_metrics_timing_only() {
        let base = RunMetrics { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15, ..Default::default() };
        let timing = TimingPatch {
            first_token_ms: Some(200),
            llm_duration_ms: Some(1000),
            execution_duration_ms: None,
            end_to_end_ms: None,
        };
        let result = merge_metrics(base, None, Some(timing));
        assert_eq!(result.prompt_tokens, 10);
        assert_eq!(result.first_token_ms, Some(200));
        assert_eq!(result.llm_duration_ms, Some(1000));
    }

    #[test]
    fn merge_metrics_auto_total() {
        let base = RunMetrics::default();
        let usage = TokenUsagePatch {
            prompt_tokens: Some(300),
            completion_tokens: Some(200),
            total_tokens: None,
        };
        let result = merge_metrics(base, Some(usage), None);
        assert_eq!(result.total_tokens, 500);
    }

    #[test]
    fn issue_cache_write_read_roundtrip() {
        let conn = test_conn();
        let now = crabitat_core::now_ms();

        // Insert a repo first (FK constraint)
        conn.execute(
            "INSERT INTO repos (repo_id, owner, name, local_path, created_at_ms) VALUES ('r1', 'o', 'n', '/tmp', ?1)",
            params![now as i64],
        ).unwrap();

        let issues = vec![
            GhIssue { number: 1, title: "Bug".into(), body: "fix it".into(), labels: vec!["bug".into()], state: "OPEN".into() },
            GhIssue { number: 2, title: "Feat".into(), body: "add it".into(), labels: vec![], state: "OPEN".into() },
        ];
        write_issues_cache(&conn, "r1", &issues).unwrap();

        let (cached, _ts) = read_cached_issues(&conn, "r1").unwrap();
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].number, 1);
        assert_eq!(cached[0].title, "Bug");
        assert_eq!(cached[1].number, 2);
    }

    #[test]
    fn seed_settings_inserts_prompts_path() {
        let conn = test_conn();
        seed_settings(&conn, StdPath::new("/my/prompts")).unwrap();

        let val: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'prompts_path'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(val, "/my/prompts");
    }
}
