use crabitat_control_plane::db;
use crabitat_control_plane::db::missions;
use crabitat_control_plane::db::repos;
use crabitat_control_plane::db::tasks;
use crabitat_control_plane::models::missions::CreateMissionRequest;
use crabitat_control_plane::models::tasks::CreateRunRequest;
use rusqlite::{Connection, params};

fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    db::migrate(&conn);
    conn
}

fn setup_repo_and_mission(conn: &Connection) -> (String, String) {
    let repo = repos::insert(conn, "l1x", "test", None, Some("url")).unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 1, "Test Issue", "Body"],
    )
    .unwrap();
    let req = CreateMissionRequest {
        repo_id: repo.repo_id.clone(),
        issue_number: 1,
        workflow_name: "test-wf".to_string(),
        flavor_id: None,
    };
    let mission = missions::insert_mission(conn, &req, "mission/branch").unwrap();
    (repo.repo_id, mission.mission_id)
}

#[test]
fn test_task_retry_logic() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t = tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    assert_eq!(t.retry_count, 0);
    assert_eq!(t.max_retries, 3);

    // Retry 1
    tasks::increment_task_retry(&conn, &t.task_id).unwrap();
    let updated = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(updated.task.retry_count, 1);
    assert_eq!(updated.task.status, "queued");

    // Retry 2
    tasks::increment_task_retry(&conn, &t.task_id).unwrap();
    let updated = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(updated.task.retry_count, 2);
}

#[test]
fn test_sticky_distribution() {
    let conn = test_conn();

    let repo = repos::insert(&conn, "l1x", "test", None, Some("url")).unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 1, "Test Issue 1", "Body 1"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 2, "Test Issue 2", "Body 2"],
    )
    .unwrap();

    let m1 = missions::insert_mission(
        &conn,
        &CreateMissionRequest {
            repo_id: repo.repo_id.clone(),
            issue_number: 1,
            workflow_name: "wf1".to_string(),
            flavor_id: None,
        },
        "branch1",
    )
    .unwrap();

    let m2 = missions::insert_mission(
        &conn,
        &CreateMissionRequest {
            repo_id: repo.repo_id.clone(),
            issue_number: 2,
            workflow_name: "wf2".to_string(),
            flavor_id: None,
        },
        "branch2",
    )
    .unwrap();

    let t1 = tasks::insert_task(&conn, &m1.mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let t2 = tasks::insert_task(&conn, &m2.mission_id, "step1", 0, "p2", 3, "queued").unwrap();

    // Pull T1 with worker-A
    let pulled = tasks::get_next_queued_task(&conn, Some("worker-A")).unwrap().unwrap();
    assert_eq!(pulled.task.task_id, t1.task_id);

    let m1_updated = missions::get_mission(&conn, &m1.mission_id).unwrap().unwrap();
    assert_eq!(m1_updated.last_worker_id, Some("worker-A".to_string()));

    // Reset T1 to queued
    tasks::update_task_status(&conn, &t1.task_id, "queued").unwrap();

    // Pull with worker-B — T1 is older so it should still be first
    let pulled_b = tasks::get_next_queued_task(&conn, Some("worker-B")).unwrap().unwrap();
    assert_eq!(pulled_b.task.task_id, t1.task_id);

    let m1_updated_b = missions::get_mission(&conn, &m1.mission_id).unwrap().unwrap();
    assert_eq!(m1_updated_b.last_worker_id, Some("worker-B".to_string()));

    // Reset T1 to queued again
    tasks::update_task_status(&conn, &t1.task_id, "queued").unwrap();

    // Set M2.last_worker_id = 'worker-A' for sticky test
    conn.execute(
        "UPDATE missions SET last_worker_id = 'worker-A' WHERE mission_id = ?1",
        [&m2.mission_id],
    )
    .unwrap();

    // Pull with worker-A — T2 should be prioritized (sticky) even though T1 is older
    let pulled_a = tasks::get_next_queued_task(&conn, Some("worker-A")).unwrap().unwrap();
    assert_eq!(pulled_a.task.task_id, t2.task_id);
}

#[test]
fn test_next_queued_task_with_null_repo_url() {
    let conn = test_conn();

    let repo = repos::insert(&conn, "l1x", "local-only", Some("/tmp/repo"), None).unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 1, "Test Issue", "Body"],
    )
    .unwrap();

    let req = CreateMissionRequest {
        repo_id: repo.repo_id.clone(),
        issue_number: 1,
        workflow_name: "wf".to_string(),
        flavor_id: None,
    };
    let mission = missions::insert_mission(&conn, &req, "branch").unwrap();
    tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "prompt", 3, "queued").unwrap();

    let result = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result.is_some());
    let task_with_git = result.unwrap();
    assert!(task_with_git.git.repo_url.is_none());
    assert_eq!(task_with_git.git.local_path, Some("/tmp/repo".to_string()));
}

#[test]
fn test_next_queued_task_with_both_url_and_local_path() {
    let conn = test_conn();

    let repo = repos::insert(&conn, "l1x", "both", Some("/tmp/repo"), Some("https://github.com/l1x/both")).unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 1, "Test", "Body"],
    )
    .unwrap();

    let req = CreateMissionRequest {
        repo_id: repo.repo_id.clone(),
        issue_number: 1,
        workflow_name: "wf".to_string(),
        flavor_id: None,
    };
    let mission = missions::insert_mission(&conn, &req, "branch").unwrap();
    tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "prompt", 3, "queued").unwrap();

    let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(result.git.repo_url, Some("https://github.com/l1x/both".to_string()));
    assert_eq!(result.git.local_path, Some("/tmp/repo".to_string()));
}

#[test]
fn test_next_queued_task_skips_deleted_repos() {
    let conn = test_conn();

    let repo = repos::insert(&conn, "l1x", "deleteme", None, Some("url")).unwrap();
    conn.execute(
        "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![repo.repo_id, 1, "Test", "Body"],
    )
    .unwrap();

    let req = CreateMissionRequest {
        repo_id: repo.repo_id.clone(),
        issue_number: 1,
        workflow_name: "wf".to_string(),
        flavor_id: None,
    };
    let mission = missions::insert_mission(&conn, &req, "branch").unwrap();
    tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "prompt", 3, "queued").unwrap();

    repos::delete(&conn, &repo.repo_id).unwrap();

    let result = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result.is_none(), "should not return tasks for deleted repos");
}

#[test]
fn test_insert_and_list_runs() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let task = tasks::insert_task(&conn, &mission_id, "step1", 0, "prompt", 3, "queued").unwrap();

    // No runs initially
    let runs = tasks::list_runs_for_task(&conn, &task.task_id).unwrap();
    assert!(runs.is_empty());

    // Insert a run
    let run_req = CreateRunRequest {
        status: "completed".to_string(),
        logs: Some("log output".to_string()),
        summary: None,
        duration_ms: Some(1500),
        tokens_used: Some(500),
    };
    tasks::insert_run(&conn, &task.task_id, &run_req).unwrap();

    let runs = tasks::list_runs_for_task(&conn, &task.task_id).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "completed");
    assert_eq!(runs[0].logs, Some("log output".to_string()));
    assert_eq!(runs[0].duration_ms, Some(1500));
    assert_eq!(runs[0].tokens_used, Some(500));
}

// --- Task gating tests ---

#[test]
fn test_blocked_task_not_returned_by_get_next_queued() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    // First task queued, second blocked
    tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    tasks::insert_task(&conn, &mission_id, "step2", 1, "p2", 3, "blocked").unwrap();

    // Only the queued task should be returned
    let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(result.task.step_id, "step1");
    assert_eq!(result.task.status, "queued");

    // Complete step1, verify blocked task is still not returned
    tasks::update_task_status(&conn, &result.task.task_id, "completed").unwrap();
    let result2 = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result2.is_none(), "blocked task should not be returned");
}

#[test]
fn test_get_task() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t = tasks::insert_task(&conn, &mission_id, "step1", 0, "my prompt", 3, "queued").unwrap();

    let fetched = tasks::get_task(&conn, &t.task_id).unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.task_id, t.task_id);
    assert_eq!(fetched.step_id, "step1");
    assert_eq!(fetched.assembled_prompt, "my prompt");
    assert_eq!(fetched.status, "queued");

    // Nonexistent task
    let none = tasks::get_task(&conn, "nonexistent").unwrap();
    assert!(none.is_none());
}

#[test]
fn test_get_next_task_in_mission() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    tasks::insert_task(&conn, &mission_id, "step2", 1, "p2", 3, "blocked").unwrap();
    tasks::insert_task(&conn, &mission_id, "step3", 2, "p3", 3, "blocked").unwrap();

    // After step_order 0, next should be step2
    let next = tasks::get_next_task_in_mission(&conn, &mission_id, 0).unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().step_id, "step2");

    // After step_order 1, next should be step3
    let next = tasks::get_next_task_in_mission(&conn, &mission_id, 1).unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().step_id, "step3");

    // After step_order 2, no more tasks
    let next = tasks::get_next_task_in_mission(&conn, &mission_id, 2).unwrap();
    assert!(next.is_none());
}

#[test]
fn test_update_task_assembled_prompt() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t = tasks::insert_task(&conn, &mission_id, "step1", 0, "original prompt", 3, "queued").unwrap();

    tasks::update_task_assembled_prompt(&conn, &t.task_id, "updated prompt with context").unwrap();

    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.assembled_prompt, "updated prompt with context");
}

#[test]
fn test_single_step_queued() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    // Single task should be queued, not blocked
    let t = tasks::insert_task(&conn, &mission_id, "only-step", 0, "prompt", 3, "queued").unwrap();
    assert_eq!(t.status, "queued");

    let result = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().task.step_id, "only-step");
}

#[test]
fn test_promote_blocked_to_queued() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t1 = tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    let t2 = tasks::insert_task(&conn, &mission_id, "step2", 1, "p2", 3, "blocked").unwrap();

    // Complete step1
    tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();

    // Manually promote step2 (simulating what the handler does)
    tasks::update_task_status(&conn, &t2.task_id, "queued").unwrap();

    // Now step2 should be returned
    let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(result.task.task_id, t2.task_id);
    assert_eq!(result.task.status, "queued");
}

#[test]
fn test_failed_leaves_downstream_blocked() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t1 = tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    let _t2 = tasks::insert_task(&conn, &mission_id, "step2", 1, "p2", 3, "blocked").unwrap();

    // Fail step1 — step2 should remain blocked
    tasks::update_task_status(&conn, &t1.task_id, "failed").unwrap();

    let next = tasks::get_next_task_in_mission(&conn, &mission_id, 0).unwrap().unwrap();
    assert_eq!(next.status, "blocked", "downstream task should remain blocked after failure");

    // No queued tasks available
    let result = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_three_step_sequential_gating() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t1 = tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    let t2 = tasks::insert_task(&conn, &mission_id, "step2", 1, "p2", 3, "blocked").unwrap();
    let t3 = tasks::insert_task(&conn, &mission_id, "step3", 2, "p3", 3, "blocked").unwrap();

    // Only step1 is queued
    let tasks_list = tasks::list_tasks_for_mission(&conn, &mission_id).unwrap();
    assert_eq!(tasks_list.len(), 3);
    assert_eq!(tasks_list[0].status, "queued");
    assert_eq!(tasks_list[1].status, "blocked");
    assert_eq!(tasks_list[2].status, "blocked");

    // Complete step1, promote step2
    tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();
    tasks::update_task_status(&conn, &t2.task_id, "queued").unwrap();

    let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(result.task.task_id, t2.task_id);

    // Step3 still blocked
    let t3_fetched = tasks::get_task(&conn, &t3.task_id).unwrap().unwrap();
    assert_eq!(t3_fetched.status, "blocked");

    // Complete step2, promote step3
    tasks::update_task_status(&conn, &t2.task_id, "completed").unwrap();
    tasks::update_task_status(&conn, &t3.task_id, "queued").unwrap();

    let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
    assert_eq!(result.task.task_id, t3.task_id);

    // Complete step3 — no more tasks
    tasks::update_task_status(&conn, &t3.task_id, "completed").unwrap();
    let result = tasks::get_next_queued_task(&conn, None).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_retry_failed_task_resets_to_queued() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t = tasks::insert_task(&conn, &mission_id, "step1", 0, "p1", 3, "queued").unwrap();
    assert_eq!(t.retry_count, 0);

    // Fail the task
    tasks::update_task_status(&conn, &t.task_id, "failed").unwrap();
    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.status, "failed");

    // Retry should reset to queued and bump retry_count
    tasks::increment_task_retry(&conn, &t.task_id).unwrap();
    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.status, "queued");
    assert_eq!(fetched.retry_count, 1);

    // Second retry
    tasks::update_task_status(&conn, &t.task_id, "failed").unwrap();
    tasks::increment_task_retry(&conn, &t.task_id).unwrap();
    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.status, "queued");
    assert_eq!(fetched.retry_count, 2);
}

#[test]
fn test_retry_updates_assembled_prompt() {
    let conn = test_conn();
    let (_, mission_id) = setup_repo_and_mission(&conn);

    let t = tasks::insert_task(&conn, &mission_id, "step1", 0, "original prompt", 3, "queued").unwrap();

    // Fail the task
    tasks::update_task_status(&conn, &t.task_id, "failed").unwrap();

    // Update assembled prompt with new context (simulating what the handler does)
    tasks::update_task_assembled_prompt(&conn, &t.task_id, "prompt with human guidance").unwrap();

    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.assembled_prompt, "prompt with human guidance");
    assert_eq!(fetched.status, "failed"); // prompt update doesn't change status

    // Now retry
    tasks::increment_task_retry(&conn, &t.task_id).unwrap();
    let fetched = tasks::get_task(&conn, &t.task_id).unwrap().unwrap();
    assert_eq!(fetched.status, "queued");
    assert_eq!(fetched.assembled_prompt, "prompt with human guidance");
    assert_eq!(fetched.retry_count, 1);
}
