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

    /// Helper: seed a repo + issue cache for mission tests
    fn setup_repo_and_issue(conn: &Connection) -> crate::models::repos::Repo {
        let repo = repos::insert(conn, "l1x", "test", None, Some("url")).unwrap();
        conn.execute(
            "INSERT INTO github_issues_cache (repo_id, number, title, body) VALUES (?1, ?2, ?3, ?4)",
            params![repo.repo_id, 1, "Test Issue", "Body"],
        )
        .unwrap();
        repo
    }

    fn make_mission_req(repo_id: &str) -> CreateMissionRequest {
        CreateMissionRequest {
            repo_id: repo_id.to_string(),
            issue_number: 1,
            workflow_name: "test-wf".to_string(),
            flavor_id: None,
        }
    }

    #[test]
    fn test_mission_status_recalculation() {
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

        // 4. Add tasks
        let t1 = tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3).unwrap();
        let t2 = tasks::insert_task(&conn, &mission.mission_id, "step2", 1, "p2", 3).unwrap();

        // Initial state: both queued -> mission pending
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "pending");

        // Step 1 running -> mission running
        tasks::update_task_status(&conn, &t1.task_id, "running").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "running");

        // Step 1 completed, Step 2 queued -> mission pending (as per current logic)
        tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "pending");

        // Step 2 failed -> mission failed
        tasks::update_task_status(&conn, &t2.task_id, "failed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "failed");

        // Step 2 retried and completed -> mission completed
        tasks::update_task_status(&conn, &t2.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id)
            .unwrap()
            .unwrap();
        assert_eq!(m.status, "completed");
    }

    #[test]
    fn test_state_history_tracks_transitions() {
        let conn = test_conn();
        let repo = setup_repo_and_issue(&conn);
        let mission = missions::insert_mission(&conn, &make_mission_req(&repo.repo_id), "b").unwrap();

        // Seed initial pending state
        missions::insert_state_history_entry(&conn, &mission.mission_id, "pending").unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].state, "pending");
        assert!(history[0].exited_at.is_none());

        // Simulate transition: pending -> running
        missions::close_current_state(&conn, &mission.mission_id).unwrap();
        missions::insert_state_history_entry(&conn, &mission.mission_id, "running").unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].state, "pending");
        assert!(history[0].exited_at.is_some(), "pending row should have exited_at set");
        assert_eq!(history[1].state, "running");
        assert!(history[1].exited_at.is_none());

        // Simulate transition: running -> completed
        missions::close_current_state(&conn, &mission.mission_id).unwrap();
        missions::insert_state_history_entry(&conn, &mission.mission_id, "completed").unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[2].state, "completed");
        assert!(history[2].exited_at.is_none(), "active state should have no exited_at");
        assert!(history[1].exited_at.is_some(), "running row should be closed");
    }

    #[test]
    fn test_recalculate_populates_state_history() {
        let conn = test_conn();
        let repo = setup_repo_and_issue(&conn);
        let mission = missions::insert_mission(&conn, &make_mission_req(&repo.repo_id), "b").unwrap();

        // Seed initial state like the handler does
        missions::insert_state_history_entry(&conn, &mission.mission_id, "pending").unwrap();

        let t1 = tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3).unwrap();

        // pending -> running
        tasks::update_task_status(&conn, &t1.task_id, "running").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].state, "pending");
        assert!(history[0].exited_at.is_some());
        assert_eq!(history[1].state, "running");

        // running -> completed
        tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[2].state, "completed");
        assert!(history[2].exited_at.is_none());
    }

    #[test]
    fn test_recalculate_no_duplicate_on_same_status() {
        let conn = test_conn();
        let repo = setup_repo_and_issue(&conn);
        let mission = missions::insert_mission(&conn, &make_mission_req(&repo.repo_id), "b").unwrap();
        missions::insert_state_history_entry(&conn, &mission.mission_id, "pending").unwrap();

        let _t1 = tasks::insert_task(&conn, &mission.mission_id, "step1", 0, "p1", 3).unwrap();
        let _t2 = tasks::insert_task(&conn, &mission.mission_id, "step2", 1, "p2", 3).unwrap();

        // Both queued -> pending. Calling recalculate twice should not add duplicate rows.
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();

        let history = missions::get_state_history(&conn, &mission.mission_id).unwrap();
        assert_eq!(history.len(), 1, "should not add duplicate pending rows");
    }

    #[test]
    fn test_get_state_history_empty_for_unknown_mission() {
        let conn = test_conn();
        let history = missions::get_state_history(&conn, "nonexistent").unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_list_all_missions() {
        let conn = test_conn();
        let repo = setup_repo_and_issue(&conn);

        assert!(missions::list_all(&conn).unwrap().is_empty());

        missions::insert_mission(&conn, &make_mission_req(&repo.repo_id), "b1").unwrap();
        assert_eq!(missions::list_all(&conn).unwrap().len(), 1);
    }

    #[test]
    fn test_list_by_repo() {
        let conn = test_conn();
        let repo = setup_repo_and_issue(&conn);

        missions::insert_mission(&conn, &make_mission_req(&repo.repo_id), "b1").unwrap();

        assert_eq!(missions::list_by_repo(&conn, &repo.repo_id).unwrap().len(), 1);
        assert!(missions::list_by_repo(&conn, "other-repo").unwrap().is_empty());
    }
}
