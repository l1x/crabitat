#[cfg(test)]
mod tests {
    use crate::db;
    use crate::db::missions;
    use crate::db::repos;
    use crate::db::tasks;
    use crate::models::missions::CreateMissionRequest;
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
        let t = tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3).unwrap();
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
        let t1 = tasks::insert_task(&conn, &m1.mission_id, "step1", 0, "p1", 3).unwrap();
        // Ensure t2 is created slightly later to test age-based fallthrough
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let t2 = tasks::insert_task(&conn, &m2.mission_id, "step1", 0, "p2", 3).unwrap();

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
}
