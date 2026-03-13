#[cfg(test)]
mod tests {
    use crate::db;
    use crate::db::missions;
    use crate::db::repos;
    use crate::db::tasks;
    use crate::models::missions::CreateMissionRequest;
    use crate::models::tasks::CreateRunRequest;
    use rusqlite::{Connection, params};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        db::migrate(&conn);
        conn
    }

    #[test]
    fn test_task_retry_logic() {
        let conn = test_conn();

        // 1. Setup repo
        let repo = repos::insert(&conn, "l1x", "test", None, Some("url")).unwrap();

        // 2. Seed issue cache (required for mission FK)
        conn.execute(
            "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo.repo_id, 1, "Test Issue", "Body"]
        ).unwrap();

        // 3. Setup mission
        let req = CreateMissionRequest {
            repo_id: repo.repo_id.clone(),
            issue_number: 1,
            workflow_name: "test-wf".to_string(),
            flavor_id: None,
        };
        let mission = missions::insert_mission(&conn, &req, "mission/branch").unwrap();

        // 4. Add task with 3 retries
        let t =
            tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3, "queued").unwrap();
        assert_eq!(t.retry_count, 0);
        assert_eq!(t.max_retries, 3);

        // 5. Trigger retry 1
        tasks::increment_task_retry(&conn, &t.task_id).unwrap();
        let updated = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(updated.task.retry_count, 1);
        assert_eq!(updated.task.status, "queued");

        // 6. Trigger retry 2
        tasks::increment_task_retry(&conn, &t.task_id).unwrap();
        let updated = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(updated.task.retry_count, 2);
    }

    #[test]
    fn test_sticky_distribution() {
        let conn = test_conn();

        // 1. Setup repo
        let repo = repos::insert(&conn, "l1x", "test", None, Some("url")).unwrap();

        // 2. Seed issue cache
        conn.execute(
            "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo.repo_id, 1, "Test Issue 1", "Body 1"]
        ).unwrap();
        conn.execute(
            "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo.repo_id, 2, "Test Issue 2", "Body 2"]
        ).unwrap();

        // 3. Setup two missions
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

        // 4. Add tasks (T1 and then T2)
        let t1 = tasks::insert_task(&conn, &m1.mission_id, "step1", 0, "p1", 3, "queued").unwrap();
        // Ensure t2 is created slightly later to test age-based fallthrough
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let t2 = tasks::insert_task(&conn, &m2.mission_id, "step1", 0, "p2", 3, "queued").unwrap();

        // 5. Pull T1 with worker-A (this will set M1.last_worker_id = 'worker-A')
        let pulled = tasks::get_next_queued_task(&conn, Some("worker-A"))
            .unwrap()
            .unwrap();
        assert_eq!(pulled.task.task_id, t1.task_id);

        // Verify last_worker_id was updated
        let m1_updated = missions::get_mission(&conn, &m1.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m1_updated.last_worker_id, Some("worker-A".to_string()));

        // 6. Reset T1 to queued for further testing
        tasks::update_task_status(&conn, &t1.task_id, "queued").unwrap();

        // 7. Pull again with worker-B
        // Since T1 is older, it should still be pulled first if no stickiness for worker-B
        let pulled_b = tasks::get_next_queued_task(&conn, Some("worker-B"))
            .unwrap()
            .unwrap();
        assert_eq!(pulled_b.task.task_id, t1.task_id);

        // Now M1.last_worker_id should be 'worker-B'
        let m1_updated_b = missions::get_mission(&conn, &m1.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m1_updated_b.last_worker_id, Some("worker-B".to_string()));

        // 8. Reset T1 to queued again
        tasks::update_task_status(&conn, &t1.task_id, "queued").unwrap();

        // 9. Set M2.last_worker_id = 'worker-A' manually for testing
        conn.execute(
            "UPDATE missions SET last_worker_id = 'worker-A' WHERE mission_id = ?1",
            [&m2.mission_id],
        )
        .unwrap();

        // 10. Pull with worker-A. T2 should be prioritized even though T1 is older.
        let pulled_a = tasks::get_next_queued_task(&conn, Some("worker-A"))
            .unwrap()
            .unwrap();
        assert_eq!(pulled_a.task.task_id, t2.task_id);
    }

    #[test]
    fn test_next_queued_task_with_null_repo_url() {
        let conn = test_conn();

        // Create repo with no repo_url (local_path only)
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
        tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt",
            3,
            "queued",
        )
        .unwrap();

        // Should succeed even with repo_url = NULL
        let result = tasks::get_next_queued_task(&conn, None).unwrap();
        assert!(result.is_some());
        let task_with_git = result.unwrap();
        assert!(task_with_git.git.repo_url.is_none());
        assert_eq!(task_with_git.git.local_path, Some("/tmp/repo".to_string()));
    }

    #[test]
    fn test_next_queued_task_with_both_url_and_local_path() {
        let conn = test_conn();

        let repo = repos::insert(
            &conn,
            "l1x",
            "both",
            Some("/tmp/repo"),
            Some("https://github.com/l1x/both"),
        )
        .unwrap();

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
        tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt",
            3,
            "queued",
        )
        .unwrap();

        let result = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(
            result.git.repo_url,
            Some("https://github.com/l1x/both".to_string())
        );
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
        tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt",
            3,
            "queued",
        )
        .unwrap();

        // Soft-delete the repo
        repos::delete(&conn, &repo.repo_id).unwrap();

        // Should return None — task exists but repo is deleted
        let result = tasks::get_next_queued_task(&conn, None).unwrap();
        assert!(
            result.is_none(),
            "should not return tasks for deleted repos"
        );
    }

    #[test]
    fn test_insert_and_list_runs() {
        let conn = test_conn();

        let repo = repos::insert(&conn, "l1x", "test", None, Some("url")).unwrap();
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
        let task = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt",
            3,
            "queued",
        )
        .unwrap();

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

    // ── Task gating & context hydration tests (issue #71) ────────────

    fn setup_mission(
        conn: &Connection,
    ) -> (crate::models::repos::Repo, crate::models::missions::Mission) {
        let repo = repos::insert(conn, "l1x", "test-gating", None, Some("url")).unwrap();
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
        let mission = missions::insert_mission(conn, &req, "mission/issue-1").unwrap();
        (repo, mission)
    }

    #[test]
    fn test_blocked_task_not_returned_by_get_next_queued() {
        // AC1: Task B cannot be picked up by a worker while Task A is running/queued
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        // Task A: queued, Task B: blocked
        let task_a = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt-a",
            3,
            "queued",
        )
        .unwrap();
        let _task_b = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step2",
            1,
            "prompt-b",
            3,
            "blocked",
        )
        .unwrap();

        // Only Task A should be returned
        let next = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(next.task.task_id, task_a.task_id);
        assert_eq!(next.task.status, "queued");

        // Mark Task A as running, Task B should still not be returned
        tasks::update_task_status(&conn, &task_a.task_id, "running").unwrap();
        let next = tasks::get_next_queued_task(&conn, None).unwrap();
        assert!(
            next.is_none(),
            "blocked task should not be returned while Task A is running"
        );
    }

    #[test]
    fn test_get_next_task_in_mission() {
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let _t1 =
            tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3, "queued").unwrap();
        let t2 =
            tasks::insert_task(&conn, &mission.mission_id, "step2", 1, "p2", 3, "blocked").unwrap();
        let t3 =
            tasks::insert_task(&conn, &mission.mission_id, "step3", 2, "p3", 3, "blocked").unwrap();

        // Next after step 0 should be step 1
        let next = tasks::get_next_task_in_mission(&conn, &mission.mission_id, 0)
            .unwrap()
            .unwrap();
        assert_eq!(next.task_id, t2.task_id);
        assert_eq!(next.step_order, 1);

        // Next after step 1 should be step 2
        let next = tasks::get_next_task_in_mission(&conn, &mission.mission_id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(next.task_id, t3.task_id);
        assert_eq!(next.step_order, 2);

        // Next after step 2 should be None
        let next = tasks::get_next_task_in_mission(&conn, &mission.mission_id, 2).unwrap();
        assert!(next.is_none());
    }

    #[test]
    fn test_promote_blocked_task_to_queued_on_completion() {
        // AC2: When Task A completes, next blocked task should be promotable
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let task_a = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt-a",
            3,
            "queued",
        )
        .unwrap();
        let task_b = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step2",
            1,
            "prompt-b",
            3,
            "blocked",
        )
        .unwrap();

        // Complete Task A
        tasks::update_task_status(&conn, &task_a.task_id, "completed").unwrap();

        // Simulate promotion: find next blocked task and promote it
        let next = tasks::get_next_task_in_mission(&conn, &mission.mission_id, task_a.step_order)
            .unwrap()
            .unwrap();
        assert_eq!(next.task_id, task_b.task_id);
        assert_eq!(next.status, "blocked");

        // Update prompt with context and promote
        tasks::update_task_assembled_prompt(&conn, &next.task_id, "prompt-b with context").unwrap();
        tasks::update_task_status(&conn, &next.task_id, "queued").unwrap();

        // Task B should now be returned by get_next_queued_task
        let queued = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(queued.task.task_id, task_b.task_id);
        assert_eq!(queued.task.assembled_prompt, "prompt-b with context");
    }

    #[test]
    fn test_failed_task_leaves_downstream_blocked() {
        // AC3: Failed Task A leaves Task B as blocked
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let task_a = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt-a",
            3,
            "queued",
        )
        .unwrap();
        let task_b = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step2",
            1,
            "prompt-b",
            3,
            "blocked",
        )
        .unwrap();

        // Fail Task A
        tasks::update_task_status(&conn, &task_a.task_id, "failed").unwrap();

        // Task B should remain blocked
        let next_task = tasks::get_task(&conn, &task_b.task_id).unwrap().unwrap();
        assert_eq!(
            next_task.status, "blocked",
            "downstream task should remain blocked when upstream fails"
        );

        // No queued tasks should be available
        let next = tasks::get_next_queued_task(&conn, None).unwrap();
        assert!(
            next.is_none(),
            "no tasks should be available when Task A failed and Task B is blocked"
        );

        // Mission should be failed
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "failed");
    }

    #[test]
    fn test_single_step_task_created_as_queued() {
        // AC4: Single-step workflows continue to work (task created as queued)
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        // Single task: should be queued
        let task = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "only-step",
            0,
            "prompt",
            3,
            "queued",
        )
        .unwrap();
        assert_eq!(task.status, "queued");

        // It should be returned by get_next_queued_task
        let next = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(next.task.task_id, task.task_id);
    }

    #[test]
    fn test_context_hydration_updates_assembled_prompt() {
        // AC2 (DB-level): Verify assembled_prompt is updated with context
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let task_a = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "prompt-a",
            3,
            "queued",
        )
        .unwrap();
        let task_b = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step2",
            1,
            "prompt-b {{context}}",
            3,
            "blocked",
        )
        .unwrap();

        // Add a run with output for Task A
        let run_req = CreateRunRequest {
            status: "completed".to_string(),
            logs: Some("Task A output: generated code".to_string()),
            summary: None,
            duration_ms: Some(1000),
            tokens_used: Some(100),
        };
        tasks::insert_run(&conn, &task_a.task_id, &run_req).unwrap();

        // Simulate extracting context from latest run
        let runs = tasks::list_runs_for_task(&conn, &task_a.task_id).unwrap();
        let context = runs.first().and_then(|r| r.logs.as_ref()).unwrap();
        assert_eq!(context, "Task A output: generated code");

        // Update Task B's prompt with context
        let new_prompt = task_b.assembled_prompt.replace("{{context}}", context);
        tasks::update_task_assembled_prompt(&conn, &task_b.task_id, &new_prompt).unwrap();

        // Verify Task B now has the hydrated prompt
        let updated_b = tasks::get_task(&conn, &task_b.task_id).unwrap().unwrap();
        assert!(
            updated_b
                .assembled_prompt
                .contains("Task A output: generated code"),
            "Task B's prompt should contain Task A's output"
        );
    }

    #[test]
    fn test_three_step_sequential_gating() {
        // Verify the A -> B -> C chain works correctly
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let t1 =
            tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3, "queued").unwrap();
        let t2 =
            tasks::insert_task(&conn, &mission.mission_id, "step2", 1, "p2", 3, "blocked").unwrap();
        let t3 =
            tasks::insert_task(&conn, &mission.mission_id, "step3", 2, "p3", 3, "blocked").unwrap();

        // Only T1 is available
        let next = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(next.task.task_id, t1.task_id);

        // Complete T1, promote T2
        tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();
        tasks::update_task_status(&conn, &t2.task_id, "queued").unwrap();

        // Now T2 is available, T3 still blocked
        let next = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(next.task.task_id, t2.task_id);
        let t3_check = tasks::get_task(&conn, &t3.task_id).unwrap().unwrap();
        assert_eq!(t3_check.status, "blocked");

        // Complete T2, promote T3
        tasks::update_task_status(&conn, &t2.task_id, "completed").unwrap();
        tasks::update_task_status(&conn, &t3.task_id, "queued").unwrap();

        // Now T3 is available
        let next = tasks::get_next_queued_task(&conn, None).unwrap().unwrap();
        assert_eq!(next.task.task_id, t3.task_id);

        // Complete T3, mission is complete
        tasks::update_task_status(&conn, &t3.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "completed");
    }

    #[test]
    fn test_get_task_returns_correct_task() {
        let conn = test_conn();
        let (_repo, mission) = setup_mission(&conn);

        let task = tasks::insert_task(
            &conn,
            &mission.mission_id,
            "step1",
            0,
            "my-prompt",
            3,
            "queued",
        )
        .unwrap();

        let fetched = tasks::get_task(&conn, &task.task_id).unwrap().unwrap();
        assert_eq!(fetched.task_id, task.task_id);
        assert_eq!(fetched.assembled_prompt, "my-prompt");
        assert_eq!(fetched.status, "queued");

        // Non-existent task
        let missing = tasks::get_task(&conn, "nonexistent").unwrap();
        assert!(missing.is_none());
    }
}
