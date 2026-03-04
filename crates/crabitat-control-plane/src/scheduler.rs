use crate::db::*;
use crate::error::ApiError;
use crate::workflows::WorkflowRegistry;
use crabitat_core::{MissionStatus, TaskId, TaskStatus, WorkflowManifest, evaluate_condition, now_ms};
use rusqlite::{Connection, params};
use std::collections::HashMap;

pub(crate) fn run_scheduler_tick_db(
    conn: &Connection,
) -> Result<(), ApiError> {
    let now = now_ms();

    // Get all queued tasks (ordered by created_at_ms)
    let mut task_stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, step_id, prompt, context
        FROM tasks
        WHERE status = 'queued'
        ORDER BY created_at_ms ASC
        ",
    )?;

    struct QueuedTask {
        task_id: String,
        mission_id: String,
        #[allow(dead_code)]
        title: String,
        step_id: Option<String>,
        #[allow(dead_code)]
        prompt: Option<String>,
        #[allow(dead_code)]
        context: Option<String>,
    }

    let queued_tasks: Vec<QueuedTask> = task_stmt
        .query_map([], |row| {
            Ok(QueuedTask {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                title: row.get(2)?,
                step_id: row.get(3)?,
                prompt: row.get(4)?,
                context: row.get(5)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    // Get all idle crabs
    let mut crab_stmt = conn.prepare("SELECT crab_id FROM crabs WHERE state = 'idle'")?;

    let mut idle_crabs: Vec<String> = crab_stmt
        .query_map([], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    for task in &queued_tasks {
        if idle_crabs.is_empty() {
            break;
        }

        // Skip merge-wait tasks — handled by background poller
        if task.step_id.as_deref() == Some("merge-wait") {
            continue;
        }

        // Check that no other task in the same mission is currently Running
        // (worktree conflict prevention for workflow tasks)
        if task.step_id.is_some() {
            let running_in_mission: i64 = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status = 'running'",
                params![task.mission_id],
                |row| row.get(0),
            )?;
            if running_in_mission > 0 {
                continue;
            }
        }

        // Simple assignment: first idle crab gets the task
        let crab_id = idle_crabs.remove(0);

        // Assign the task
        conn.execute(
            "UPDATE tasks SET assigned_crab_id = ?2, status = ?3, updated_at_ms = ?4 WHERE task_id = ?1",
            params![task.task_id, crab_id, task_status_to_db(TaskStatus::Assigned), now],
        )?;

        conn.execute(
            "UPDATE crabs SET state = 'busy', current_task_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?1",
            params![crab_id, task.task_id, now],
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Sequential Mission Activation
// ---------------------------------------------------------------------------

pub(crate) fn activate_next_mission_in_repo(
    conn: &Connection,
    repo_id: &str,
    workflows: &WorkflowRegistry,
) -> Result<(), ApiError> {
    // Find ALL pending queued missions for this repo
    let mut stmt = conn.prepare(
        "SELECT mission_id, workflow_name, prompt FROM missions WHERE repo_id = ?1 AND status = 'pending' AND queue_position IS NOT NULL ORDER BY queue_position ASC",
    )?;

    let pending: Vec<(String, Option<String>, String)> = stmt
        .query_map(params![repo_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .filter_map(Result::ok)
        .collect();

    for (mission_id, workflow_name, mission_prompt) in pending {
        let worktree_path = format!("burrows/mission-{mission_id}");
        conn.execute(
            "UPDATE missions SET status = ?2, worktree_path = ?3 WHERE mission_id = ?1",
            params![mission_id, mission_status_to_db(MissionStatus::Running), worktree_path],
        )?;

        // Expand workflow into tasks if workflow is specified
        if let Some(ref wf_name) = workflow_name
            && let Some(manifest) = workflows.get(wf_name)
        {
            expand_workflow_into_tasks(
                conn,
                workflows,
                &manifest.clone(),
                &mission_id,
                &mission_prompt,
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Workflow expansion
// ---------------------------------------------------------------------------

pub(crate) fn expand_workflow_into_tasks(
    conn: &Connection,
    registry: &WorkflowRegistry,
    manifest: &WorkflowManifest,
    mission_id: &str,
    mission_prompt: &str,
) -> Result<(), ApiError> {
    let now = now_ms();

    // Map step_id -> task_id for dependency linking
    let mut step_to_task: HashMap<String, String> = HashMap::new();

    for step in &manifest.steps {
        let task_id = TaskId::new().to_string();
        let has_deps = !step.depends_on.is_empty();
        let status = if has_deps { TaskStatus::Blocked } else { TaskStatus::Queued };

        // Load and render the core prompt template
        let prompt_template = registry.load_prompt_file(&step.prompt_file).unwrap_or_default();
        let mut rendered_prompt = prompt_template
            .replace("{{mission_prompt}}", mission_prompt)
            .replace("{{context}}", "")
            .replace("{{worktree_path}}", &format!("burrows/mission-{mission_id}"));

        // Resolve effective include: step overrides workflow default
        let effective_include = step.include.as_deref().unwrap_or(&manifest.workflow.include);
        for include_path in effective_include {
            if let Ok(content) = registry.load_prompt_file(include_path) {
                rendered_prompt.push_str("\n\n---\n\n");
                rendered_prompt.push_str(&content);
            }
        }

        // Store condition and max_retries in context JSON if present
        let context_json = if step.condition.is_some() || step.max_retries > 0 {
            let mut ctx = serde_json::Map::new();
            if let Some(ref cond) = step.condition {
                ctx.insert("_condition".to_string(), serde_json::Value::String(cond.clone()));
            }
            if step.max_retries > 0 {
                ctx.insert(
                    "_max_retries".to_string(),
                    serde_json::Value::Number(step.max_retries.into()),
                );
            }
            Some(serde_json::to_string(&ctx).unwrap_or_default())
        } else {
            None
        };

        conn.execute(
            "
            INSERT INTO tasks (task_id, mission_id, title, assigned_crab_id, status,
                               step_id, prompt, context,
                               created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                task_id,
                mission_id,
                format!("[{}]", step.id),
                task_status_to_db(status),
                step.id,
                rendered_prompt,
                context_json,
                now,
                now
            ],
        )?;

        step_to_task.insert(step.id.clone(), task_id.clone());
    }

    // Insert dependency edges
    for step in &manifest.steps {
        if let Some(task_id) = step_to_task.get(&step.id) {
            for dep_step_id in &step.depends_on {
                if let Some(dep_task_id) = step_to_task.get(dep_step_id) {
                    conn.execute(
                        "INSERT INTO task_deps (task_id, depends_on_task_id) VALUES (?1, ?2)",
                        params![task_id, dep_task_id],
                    )?;
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Workflow cascade
// ---------------------------------------------------------------------------

/// After a task completes/fails, check dependent tasks and update their status.
pub(crate) fn cascade_workflow(
    conn: &Connection,
    mission_id: &str,
    completed_task_id: &str,
    workflows: &WorkflowRegistry,
) -> Result<(), ApiError> {
    let now = now_ms();

    // Get the completed task's info
    let completed_task = match fetch_task(conn, completed_task_id)? {
        Some(t) => t,
        None => return Ok(()),
    };

    // If this task has no step_id, it's not part of a workflow — skip cascade
    if completed_task.step_id.is_none() {
        return Ok(());
    }

    let completed_step_id = completed_task.step_id.as_deref().unwrap_or("");

    // If the task failed, cascade failure to all dependents
    if matches!(completed_task.status, TaskStatus::Failed) {
        cascade_failure(conn, completed_task_id, now)?;
        update_mission_status(conn, mission_id, now, workflows)?;
        return Ok(());
    }

    // Build context map from completed runs in this mission
    let context_map = build_context_map(conn, mission_id)?;

    // Find tasks that depend on the completed task
    let mut stmt = conn.prepare("SELECT task_id FROM task_deps WHERE depends_on_task_id = ?1")?;
    let dependent_task_ids: Vec<String> = stmt
        .query_map(params![completed_task_id], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    for dep_task_id in &dependent_task_ids {
        let dep_task = match fetch_task(conn, dep_task_id)? {
            Some(t) => t,
            None => continue,
        };

        // Only process blocked tasks
        if !matches!(dep_task.status, TaskStatus::Blocked) {
            continue;
        }

        // Check if ALL dependencies are terminal (Completed or Skipped)
        let blocked_count: i64 = conn.query_row(
            "
            SELECT COUNT(*) FROM task_deps td
            JOIN tasks t ON td.depends_on_task_id = t.task_id
            WHERE td.task_id = ?1 AND t.status NOT IN ('completed', 'skipped')
            ",
            params![dep_task_id],
            |row| row.get(0),
        )?;

        if blocked_count > 0 {
            continue; // Still has unresolved dependencies
        }

        // All deps done — evaluate condition
        let condition = get_task_condition(conn, dep_task_id)?;

        let should_queue =
            if let Some(cond) = condition { evaluate_condition(&cond, &context_map) } else { true };

        if should_queue {
            // Build accumulated context from dependency chain
            let accumulated_context = build_accumulated_context(conn, dep_task_id)?;

            conn.execute(
                "UPDATE tasks SET status = ?2, context = ?3, updated_at_ms = ?4 WHERE task_id = ?1",
                params![
                    dep_task_id,
                    task_status_to_db(TaskStatus::Queued),
                    accumulated_context,
                    now
                ],
            )?;
        } else {
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![dep_task_id, task_status_to_db(TaskStatus::Skipped), now],
            )?;
        }

        // If we just skipped a task, recurse to cascade further
        if !should_queue {
            cascade_workflow(conn, mission_id, dep_task_id, workflows)?;
        }
    }

    // Handle fix→review retry loop: if a "fix" step completed, find the "review"
    // step that depends on "implement" (same mission) and re-queue it
    if completed_step_id == "fix" {
        requeue_review_after_fix(conn, mission_id, now)?;
    }

    // Capture PR number from the "pr" step result
    if completed_step_id == "pr" {
        let context_map = build_context_map(conn, mission_id)?;
        if let Some(pr_num_str) = context_map.get("pr.result")
            && let Ok(pr_num) = pr_num_str.parse::<i64>()
        {
            conn.execute(
                "UPDATE missions SET github_pr_number = ?2 WHERE mission_id = ?1",
                params![mission_id, pr_num],
            )?;
        }
    }

    update_mission_status(conn, mission_id, now, workflows)?;
    Ok(())
}

fn cascade_failure(
    conn: &Connection,
    failed_task_id: &str,
    now: u64,
) -> Result<(), ApiError> {
    let mut stmt = conn.prepare("SELECT task_id FROM task_deps WHERE depends_on_task_id = ?1")?;
    let dependent_task_ids: Vec<String> =
        stmt.query_map(params![failed_task_id], |row| row.get(0))?.filter_map(Result::ok).collect();

    for dep_task_id in &dependent_task_ids {
        conn.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![dep_task_id, task_status_to_db(TaskStatus::Failed), now],
        )?;
        cascade_failure(conn, dep_task_id, now)?;
    }
    Ok(())
}

fn requeue_review_after_fix(
    conn: &Connection,
    mission_id: &str,
    now: u64,
) -> Result<(), ApiError> {
    // Find the "review" task in this mission and check its retry count
    let review_task: Option<(String, i64)> = conn
        .query_row(
            "
            SELECT task_id,
                   (SELECT COUNT(*) FROM runs WHERE task_id = t.task_id AND status = 'completed') as run_count
            FROM tasks t
            WHERE mission_id = ?1 AND step_id = 'review'
            ",
            params![mission_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((review_task_id, _run_count)) = review_task {
        // Reset review to Queued so it re-runs
        conn.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![review_task_id, task_status_to_db(TaskStatus::Queued), now],
        )?;
    }
    Ok(())
}

fn build_context_map(
    conn: &Connection,
    mission_id: &str,
) -> Result<HashMap<String, String>, ApiError> {
    let mut context: HashMap<String, String> = HashMap::new();

    let mut stmt = conn.prepare(
        "
        SELECT t.step_id, r.summary
        FROM tasks t
        JOIN runs r ON r.task_id = t.task_id
        WHERE t.mission_id = ?1 AND r.status = 'completed' AND t.step_id IS NOT NULL
        ORDER BY r.completed_at_ms DESC
        ",
    )?;

    let rows: Vec<(String, String)> = stmt
        .query_map(params![mission_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default()))
        })?
        .filter_map(Result::ok)
        .collect();

    for (step_id, summary) in rows {
        context.insert(format!("{step_id}.summary"), summary.clone());
        // Try to extract a "result" field from the summary (JSON)
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&summary)
            && let Some(result) = val.get("result").and_then(|v| v.as_str())
        {
            context.insert(format!("{step_id}.result"), result.to_string());
        }
    }

    Ok(context)
}

fn build_accumulated_context(conn: &Connection, task_id: &str) -> Result<String, ApiError> {
    // Collect summaries from all transitive dependencies
    let mut summaries = Vec::new();

    let mut stmt = conn.prepare(
        "
        SELECT t.step_id, r.summary
        FROM task_deps td
        JOIN tasks t ON td.depends_on_task_id = t.task_id
        LEFT JOIN runs r ON r.task_id = t.task_id AND r.status = 'completed'
        WHERE td.task_id = ?1
        ORDER BY t.created_at_ms ASC
        ",
    )?;

    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map(params![task_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    for (step_id, summary) in rows {
        let step = step_id.unwrap_or_else(|| "unknown".to_string());
        let sum = summary.unwrap_or_else(|| "(no summary)".to_string());
        summaries.push(format!("## {step}\n{sum}"));
    }

    Ok(summaries.join("\n\n"))
}

fn get_task_condition(conn: &Connection, task_id: &str) -> Result<Option<String>, ApiError> {
    let context: Option<String> = conn
        .query_row("SELECT context FROM tasks WHERE task_id = ?1", params![task_id], |row| {
            row.get(0)
        })
        .ok();

    if let Some(ctx) = context
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&ctx)
        && let Some(cond) = val.get("_condition").and_then(|v| v.as_str())
    {
        return Ok(Some(cond.to_string()));
    }
    Ok(None)
}

fn update_mission_status(
    conn: &Connection,
    mission_id: &str,
    _now: u64,
    workflows: &WorkflowRegistry,
) -> Result<(), ApiError> {
    // Check if all tasks in the mission are terminal
    let non_terminal_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status NOT IN ('completed', 'failed', 'skipped')",
        params![mission_id],
        |row| row.get(0),
    )?;

    if non_terminal_count == 0 {
        // Check if any task failed
        let failed_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status = 'failed'",
            params![mission_id],
            |row| row.get(0),
        )?;

        let new_status =
            if failed_count > 0 { MissionStatus::Failed } else { MissionStatus::Completed };

        conn.execute(
            "UPDATE missions SET status = ?2 WHERE mission_id = ?1",
            params![mission_id, mission_status_to_db(new_status)],
        )?;

        // Try to activate next mission in this repo's queue
        if let Ok(Some(mission)) = fetch_mission(conn, mission_id) {
            activate_next_mission_in_repo(conn, &mission.repo_id, workflows)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::apply_schema;
    use crate::workflows::WorkflowRegistry;
    use crabitat_core::{WorkflowManifest, WorkflowMeta, WorkflowStep};
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
        conn
    }

    fn test_registry() -> WorkflowRegistry {
        WorkflowRegistry {
            manifests: HashMap::new(),
            prompts_path: PathBuf::from("/tmp/test-prompts"),
            stack_map: HashMap::new(),
        }
    }

    fn seed_repo_and_mission(conn: &Connection) -> (String, String) {
        let now = crabitat_core::now_ms() as i64;
        conn.execute(
            "INSERT INTO repos (repo_id, owner, name, local_path, created_at_ms) VALUES ('r1', 'o', 'n', '/tmp', ?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO missions (mission_id, repo_id, prompt, status, created_at_ms) VALUES ('m1', 'r1', 'test', 'running', ?1)",
            params![now],
        ).unwrap();
        ("r1".into(), "m1".into())
    }

    #[test]
    fn scheduler_assigns_queued_to_idle() {
        let conn = test_conn();
        let now = crabitat_core::now_ms() as i64;
        let (_repo_id, _mission_id) = seed_repo_and_mission(&conn);

        conn.execute(
            "INSERT INTO crabs (crab_id, repo_id, name, state, updated_at_ms) VALUES ('c1', 'r1', 'Alice', 'idle', ?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, created_at_ms, updated_at_ms) VALUES ('t1', 'm1', 'Do work', 'queued', ?1, ?1)",
            params![now],
        ).unwrap();

        run_scheduler_tick_db(&conn).unwrap();

        let status: String = conn.query_row("SELECT status FROM tasks WHERE task_id = 't1'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "assigned");

        let crab_state: String = conn.query_row("SELECT state FROM crabs WHERE crab_id = 'c1'", [], |r| r.get(0)).unwrap();
        assert_eq!(crab_state, "busy");
    }

    #[test]
    fn scheduler_skips_merge_wait() {
        let conn = test_conn();
        let now = crabitat_core::now_ms() as i64;
        let (_repo_id, _mission_id) = seed_repo_and_mission(&conn);

        conn.execute(
            "INSERT INTO crabs (crab_id, repo_id, name, state, updated_at_ms) VALUES ('c1', 'r1', 'Alice', 'idle', ?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, step_id, created_at_ms, updated_at_ms) VALUES ('t1', 'm1', 'Wait for merge', 'queued', 'merge-wait', ?1, ?1)",
            params![now],
        ).unwrap();

        run_scheduler_tick_db(&conn).unwrap();

        let status: String = conn.query_row("SELECT status FROM tasks WHERE task_id = 't1'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "queued"); // not assigned
    }

    #[test]
    fn scheduler_no_crabs_does_nothing() {
        let conn = test_conn();
        let now = crabitat_core::now_ms() as i64;
        let (_repo_id, _mission_id) = seed_repo_and_mission(&conn);

        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, created_at_ms, updated_at_ms) VALUES ('t1', 'm1', 'Do work', 'queued', ?1, ?1)",
            params![now],
        ).unwrap();

        run_scheduler_tick_db(&conn).unwrap();

        let status: String = conn.query_row("SELECT status FROM tasks WHERE task_id = 't1'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "queued");
    }

    #[test]
    fn expand_workflow_creates_tasks() {
        let conn = test_conn();
        let (_repo_id, _mission_id) = seed_repo_and_mission(&conn);
        let registry = test_registry();

        let manifest = WorkflowManifest {
            workflow: WorkflowMeta {
                name: "test-wf".into(),
                description: "Test".into(),
                version: "1.0.0".into(),
                include: vec![],
            },
            steps: vec![
                WorkflowStep { id: "plan".into(), prompt_file: "plan.md".into(), depends_on: vec![], condition: None, max_retries: 0, include: None },
                WorkflowStep { id: "implement".into(), prompt_file: "impl.md".into(), depends_on: vec!["plan".into()], condition: None, max_retries: 0, include: None },
                WorkflowStep { id: "review".into(), prompt_file: "review.md".into(), depends_on: vec!["implement".into()], condition: None, max_retries: 0, include: None },
            ],
        };

        expand_workflow_into_tasks(&conn, &registry, &manifest, "m1", "do stuff").unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE mission_id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 3);

        // First task (no deps) should be queued
        let plan_status: String = conn.query_row(
            "SELECT status FROM tasks WHERE mission_id = 'm1' AND step_id = 'plan'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(plan_status, "queued");

        // Dependent tasks should be blocked
        let impl_status: String = conn.query_row(
            "SELECT status FROM tasks WHERE mission_id = 'm1' AND step_id = 'implement'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(impl_status, "blocked");
    }

    #[test]
    fn cascade_failure_propagates() {
        let conn = test_conn();
        let now = crabitat_core::now_ms() as i64;
        let (_repo_id, _mission_id) = seed_repo_and_mission(&conn);

        // Create a chain: t1 (failed) -> t2 (blocked) -> t3 (blocked)
        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, step_id, created_at_ms, updated_at_ms) VALUES ('t1', 'm1', 'Step 1', 'failed', 's1', ?1, ?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, step_id, created_at_ms, updated_at_ms) VALUES ('t2', 'm1', 'Step 2', 'blocked', 's2', ?1, ?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, mission_id, title, status, step_id, created_at_ms, updated_at_ms) VALUES ('t3', 'm1', 'Step 3', 'blocked', 's3', ?1, ?1)",
            params![now],
        ).unwrap();
        conn.execute("INSERT INTO task_deps (task_id, depends_on_task_id) VALUES ('t2', 't1')", []).unwrap();
        conn.execute("INSERT INTO task_deps (task_id, depends_on_task_id) VALUES ('t3', 't2')", []).unwrap();

        cascade_failure(&conn, "t1", now as u64).unwrap();

        let t2_status: String = conn.query_row("SELECT status FROM tasks WHERE task_id = 't2'", [], |r| r.get(0)).unwrap();
        let t3_status: String = conn.query_row("SELECT status FROM tasks WHERE task_id = 't3'", [], |r| r.get(0)).unwrap();
        assert_eq!(t2_status, "failed");
        assert_eq!(t3_status, "failed");
    }
}
