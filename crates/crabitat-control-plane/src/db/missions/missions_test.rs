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
        let m = missions::get_mission(&conn, &mission.mission_id).unwrap().unwrap();
        assert_eq!(m.status, "pending");

        // Step 1 running -> mission running
        tasks::update_task_status(&conn, &t1.task_id, "running").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id).unwrap().unwrap();
        assert_eq!(m.status, "running");

        // Step 1 completed, Step 2 queued -> mission pending (as per current logic)
        tasks::update_task_status(&conn, &t1.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id).unwrap().unwrap();
        assert_eq!(m.status, "pending"); 

        // Step 2 failed -> mission failed
        tasks::update_task_status(&conn, &t2.task_id, "failed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id).unwrap().unwrap();
        assert_eq!(m.status, "failed");

        // Step 2 retried and completed -> mission completed
        tasks::update_task_status(&conn, &t2.task_id, "completed").unwrap();
        missions::recalculate_mission_status(&conn, &mission.mission_id).unwrap();
        let m = missions::get_mission(&conn, &mission.mission_id).unwrap().unwrap();
        assert_eq!(m.status, "completed");
    }
}
