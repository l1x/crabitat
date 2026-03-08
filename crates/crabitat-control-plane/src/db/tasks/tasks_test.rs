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
        let updated = tasks::get_next_queued_task(&conn).unwrap().unwrap();
        assert_eq!(updated.task.retry_count, 1);
        assert_eq!(updated.task.status, "queued");

        // 6. Trigger retry 2
        tasks::increment_task_retry(&conn, &t.task_id).unwrap();
        let updated = tasks::get_next_queued_task(&conn).unwrap().unwrap();
        assert_eq!(updated.task.retry_count, 2);
    }
}
