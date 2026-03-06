use rusqlite::{Connection, params};

use crate::models::{
    CreateStepInput, Workflow, WorkflowDetail, WorkflowFlavor, WorkflowStep, WorkflowSummary,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn insert(
    conn: &Connection,
    repo_id: &str,
    name: &str,
    description: &str,
    steps: &[CreateStepInput],
) -> Result<WorkflowDetail, String> {
    let workflow_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO workflows (workflow_id, repo_id, name, description) VALUES (?1, ?2, ?3, ?4)",
        params![workflow_id, repo_id, name, description],
    )
    .map_err(|e| format!("workflow already exists: {e}"))?;

    insert_steps(conn, &workflow_id, steps)?;
    get_detail(conn, &workflow_id).map(|o| o.unwrap())
}

pub fn list_all(conn: &Connection) -> Result<Vec<WorkflowSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.workflow_id, w.repo_id, w.name, w.description, w.created_at,
                    r.owner, r.name,
                    (SELECT COUNT(*) FROM workflow_flavors f WHERE f.workflow_id = w.workflow_id)
             FROM workflows w
             JOIN repos r ON r.repo_id = w.repo_id
             ORDER BY w.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(WorkflowSummary {
                workflow: Workflow {
                    workflow_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                },
                repo_owner: row.get(5)?,
                repo_name: row.get(6)?,
                flavor_count: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

pub fn list_by_repo(conn: &Connection, repo_id: &str) -> Result<Vec<WorkflowSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.workflow_id, w.repo_id, w.name, w.description, w.created_at,
                    r.owner, r.name,
                    (SELECT COUNT(*) FROM workflow_flavors f WHERE f.workflow_id = w.workflow_id)
             FROM workflows w
             JOIN repos r ON r.repo_id = w.repo_id
             WHERE w.repo_id = ?1
             ORDER BY w.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![repo_id], |row| {
            Ok(WorkflowSummary {
                workflow: Workflow {
                    workflow_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                },
                repo_owner: row.get(5)?,
                repo_name: row.get(6)?,
                flavor_count: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

pub fn get_detail(conn: &Connection, workflow_id: &str) -> Result<Option<WorkflowDetail>, String> {
    let workflow = match get_workflow_row(conn, workflow_id)? {
        Some(w) => w,
        None => return Ok(None),
    };
    let steps = list_steps(conn, workflow_id)?;
    let flavors = list_flavors(conn, workflow_id)?;

    Ok(Some(WorkflowDetail {
        workflow,
        steps,
        flavors,
    }))
}

pub fn update(
    conn: &Connection,
    workflow_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    steps: Option<&[CreateStepInput]>,
) -> Result<Option<WorkflowDetail>, String> {
    // Check existence
    if get_workflow_row(conn, workflow_id)?.is_none() {
        return Ok(None);
    }

    if let Some(n) = name {
        conn.execute(
            "UPDATE workflows SET name = ?1 WHERE workflow_id = ?2",
            params![n, workflow_id],
        )
        .map_err(|e| e.to_string())?;
    }

    if let Some(d) = description {
        conn.execute(
            "UPDATE workflows SET description = ?1 WHERE workflow_id = ?2",
            params![d, workflow_id],
        )
        .map_err(|e| e.to_string())?;
    }

    if let Some(s) = steps {
        conn.execute(
            "DELETE FROM workflow_steps WHERE workflow_id = ?1",
            params![workflow_id],
        )
        .map_err(|e| e.to_string())?;
        insert_steps(conn, workflow_id, s)?;
    }

    get_detail(conn, workflow_id)
}

pub fn delete(conn: &Connection, workflow_id: &str) -> Result<bool, String> {
    let affected = conn
        .execute(
            "DELETE FROM workflows WHERE workflow_id = ?1",
            params![workflow_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(affected > 0)
}

pub fn insert_flavor(
    conn: &Connection,
    workflow_id: &str,
    name: &str,
    context: Option<&str>,
) -> Result<WorkflowFlavor, String> {
    // Check workflow exists
    if get_workflow_row(conn, workflow_id)?.is_none() {
        return Err("workflow not found".to_string());
    }

    let flavor_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO workflow_flavors (flavor_id, workflow_id, name, context) VALUES (?1, ?2, ?3, ?4)",
        params![flavor_id, workflow_id, name, context],
    )
    .map_err(|e| format!("flavor already exists: {e}"))?;

    Ok(WorkflowFlavor {
        flavor_id,
        workflow_id: workflow_id.to_string(),
        name: name.to_string(),
        context: context.map(|s| s.to_string()),
    })
}

pub fn delete_flavor(conn: &Connection, flavor_id: &str) -> Result<bool, String> {
    let affected = conn
        .execute(
            "DELETE FROM workflow_flavors WHERE flavor_id = ?1",
            params![flavor_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(affected > 0)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn insert_steps(
    conn: &Connection,
    workflow_id: &str,
    steps: &[CreateStepInput],
) -> Result<(), String> {
    for (i, step) in steps.iter().enumerate() {
        let step_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO workflow_steps (step_id, workflow_id, step_order, name, prompt_template) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![step_id, workflow_id, i as i64, step.name, step.prompt_template],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn get_workflow_row(conn: &Connection, workflow_id: &str) -> Result<Option<Workflow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT workflow_id, repo_id, name, description, created_at FROM workflows WHERE workflow_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let mut rows = stmt
        .query_map(params![workflow_id], |row| {
            Ok(Workflow {
                workflow_id: row.get(0)?,
                repo_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    match rows.next() {
        Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

fn list_steps(conn: &Connection, workflow_id: &str) -> Result<Vec<WorkflowStep>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT step_id, workflow_id, step_order, name, prompt_template
             FROM workflow_steps WHERE workflow_id = ?1 ORDER BY step_order",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![workflow_id], |row| {
            Ok(WorkflowStep {
                step_id: row.get(0)?,
                workflow_id: row.get(1)?,
                step_order: row.get(2)?,
                name: row.get(3)?,
                prompt_template: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

fn list_flavors(conn: &Connection, workflow_id: &str) -> Result<Vec<WorkflowFlavor>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT flavor_id, workflow_id, name, context
             FROM workflow_flavors WHERE workflow_id = ?1 ORDER BY name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![workflow_id], |row| {
            Ok(WorkflowFlavor {
                flavor_id: row.get(0)?,
                workflow_id: row.get(1)?,
                name: row.get(2)?,
                context: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        db::migrate(&conn);
        conn
    }

    fn seed_repo(conn: &Connection) -> String {
        crate::db::repos::insert(conn, "acme", "widgets", "/tmp/widgets")
            .unwrap()
            .repo_id
    }

    fn sample_steps() -> Vec<CreateStepInput> {
        vec![
            CreateStepInput {
                name: "Plan".to_string(),
                prompt_template: "Create a plan for {{task}}".to_string(),
            },
            CreateStepInput {
                name: "Code".to_string(),
                prompt_template: "Implement the plan".to_string(),
            },
        ]
    }

    // -----------------------------------------------------------------------
    // insert
    // -----------------------------------------------------------------------

    #[test]
    fn insert_creates_workflow_with_steps() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let detail = insert(&conn, &repo_id, "deploy", "Deploy pipeline", &sample_steps()).unwrap();

        assert_eq!(detail.workflow.name, "deploy");
        assert_eq!(detail.workflow.description, "Deploy pipeline");
        assert_eq!(detail.workflow.repo_id, repo_id);
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].name, "Plan");
        assert_eq!(detail.steps[0].step_order, 0);
        assert_eq!(detail.steps[1].name, "Code");
        assert_eq!(detail.steps[1].step_order, 1);
        assert!(detail.flavors.is_empty());
    }

    #[test]
    fn insert_with_empty_steps() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let detail = insert(&conn, &repo_id, "empty-wf", "", &[]).unwrap();

        assert_eq!(detail.steps.len(), 0);
    }

    #[test]
    fn insert_duplicate_name_same_repo_fails() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        insert(&conn, &repo_id, "deploy", "", &sample_steps()).unwrap();
        let err = insert(&conn, &repo_id, "deploy", "", &[]).unwrap_err();

        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn insert_same_name_different_repo_ok() {
        let conn = test_conn();
        let repo1 = seed_repo(&conn);
        let repo2 = crate::db::repos::insert(&conn, "acme", "gadgets", "/tmp/gadgets")
            .unwrap()
            .repo_id;

        insert(&conn, &repo1, "deploy", "", &[]).unwrap();
        insert(&conn, &repo2, "deploy", "", &[]).unwrap();
    }

    // -----------------------------------------------------------------------
    // get_detail
    // -----------------------------------------------------------------------

    #[test]
    fn get_detail_returns_none_for_missing() {
        let conn = test_conn();
        assert!(get_detail(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn get_detail_includes_steps_and_flavors() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &sample_steps()).unwrap();
        insert_flavor(&conn, &wf.workflow.workflow_id, "rust", Some("Rust context")).unwrap();

        let detail = get_detail(&conn, &wf.workflow.workflow_id).unwrap().unwrap();
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.flavors.len(), 1);
        assert_eq!(detail.flavors[0].name, "rust");
        assert_eq!(detail.flavors[0].context.as_deref(), Some("Rust context"));
    }

    // -----------------------------------------------------------------------
    // list_all / list_by_repo
    // -----------------------------------------------------------------------

    #[test]
    fn list_all_empty() {
        let conn = test_conn();
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn list_all_returns_summaries_with_repo_info() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        insert(&conn, &repo_id, "wf1", "desc1", &sample_steps()).unwrap();

        let summaries = list_all(&conn).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].workflow.name, "wf1");
        assert_eq!(summaries[0].repo_owner, "acme");
        assert_eq!(summaries[0].repo_name, "widgets");
        assert_eq!(summaries[0].flavor_count, 0);
    }

    #[test]
    fn list_all_flavor_count_reflects_flavors() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf1", "", &[]).unwrap();
        insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap();
        insert_flavor(&conn, &wf.workflow.workflow_id, "python", None).unwrap();

        let summaries = list_all(&conn).unwrap();
        assert_eq!(summaries[0].flavor_count, 2);
    }

    #[test]
    fn list_by_repo_filters_correctly() {
        let conn = test_conn();
        let repo1 = seed_repo(&conn);
        let repo2 = crate::db::repos::insert(&conn, "acme", "gadgets", "/tmp/gadgets")
            .unwrap()
            .repo_id;

        insert(&conn, &repo1, "wf-a", "", &[]).unwrap();
        insert(&conn, &repo2, "wf-b", "", &[]).unwrap();

        let list1 = list_by_repo(&conn, &repo1).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].workflow.name, "wf-a");

        let list2 = list_by_repo(&conn, &repo2).unwrap();
        assert_eq!(list2.len(), 1);
        assert_eq!(list2[0].workflow.name, "wf-b");
    }

    // -----------------------------------------------------------------------
    // update
    // -----------------------------------------------------------------------

    #[test]
    fn update_nonexistent_returns_none() {
        let conn = test_conn();
        assert!(update(&conn, "no-such-id", Some("x"), None, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn update_name_only() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "old", "desc", &sample_steps()).unwrap();

        let updated = update(&conn, &wf.workflow.workflow_id, Some("new"), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(updated.workflow.name, "new");
        assert_eq!(updated.workflow.description, "desc");
        assert_eq!(updated.steps.len(), 2);
    }

    #[test]
    fn update_description_only() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "old-desc", &[]).unwrap();

        let updated = update(
            &conn,
            &wf.workflow.workflow_id,
            None,
            Some("new-desc"),
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.workflow.name, "wf");
        assert_eq!(updated.workflow.description, "new-desc");
    }

    #[test]
    fn update_replaces_steps_wholesale() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &sample_steps()).unwrap();
        assert_eq!(wf.steps.len(), 2);

        let new_steps = vec![CreateStepInput {
            name: "Only".to_string(),
            prompt_template: "Do the thing".to_string(),
        }];
        let updated = update(
            &conn,
            &wf.workflow.workflow_id,
            None,
            None,
            Some(&new_steps),
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.steps.len(), 1);
        assert_eq!(updated.steps[0].name, "Only");
    }

    #[test]
    fn update_all_fields_at_once() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "old", &sample_steps()).unwrap();

        let new_steps = vec![CreateStepInput {
            name: "Single".to_string(),
            prompt_template: "tmpl".to_string(),
        }];
        let updated = update(
            &conn,
            &wf.workflow.workflow_id,
            Some("renamed"),
            Some("new desc"),
            Some(&new_steps),
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.workflow.name, "renamed");
        assert_eq!(updated.workflow.description, "new desc");
        assert_eq!(updated.steps.len(), 1);
    }

    // -----------------------------------------------------------------------
    // delete
    // -----------------------------------------------------------------------

    #[test]
    fn delete_existing_returns_true() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &sample_steps()).unwrap();

        assert!(delete(&conn, &wf.workflow.workflow_id).unwrap());
        assert!(get_detail(&conn, &wf.workflow.workflow_id).unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let conn = test_conn();
        assert!(!delete(&conn, "nope").unwrap());
    }

    #[test]
    fn delete_cascades_steps_and_flavors() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &sample_steps()).unwrap();
        insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap();

        delete(&conn, &wf.workflow.workflow_id).unwrap();

        // Verify steps and flavors are gone
        assert!(list_steps(&conn, &wf.workflow.workflow_id).unwrap().is_empty());
        assert!(list_flavors(&conn, &wf.workflow.workflow_id).unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // insert_flavor
    // -----------------------------------------------------------------------

    #[test]
    fn insert_flavor_ok() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &[]).unwrap();

        let flavor =
            insert_flavor(&conn, &wf.workflow.workflow_id, "python", Some("Python 3.12")).unwrap();
        assert_eq!(flavor.name, "python");
        assert_eq!(flavor.context.as_deref(), Some("Python 3.12"));
        assert_eq!(flavor.workflow_id, wf.workflow.workflow_id);
    }

    #[test]
    fn insert_flavor_without_context() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &[]).unwrap();

        let flavor = insert_flavor(&conn, &wf.workflow.workflow_id, "go", None).unwrap();
        assert!(flavor.context.is_none());
    }

    #[test]
    fn insert_flavor_nonexistent_workflow_fails() {
        let conn = test_conn();
        let err = insert_flavor(&conn, "no-such-wf", "rust", None).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn insert_flavor_duplicate_name_fails() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &[]).unwrap();

        insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap();
        let err = insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    // -----------------------------------------------------------------------
    // delete_flavor
    // -----------------------------------------------------------------------

    #[test]
    fn delete_flavor_ok() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &[]).unwrap();
        let flavor = insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap();

        assert!(delete_flavor(&conn, &flavor.flavor_id).unwrap());
        // Verify it's gone
        let detail = get_detail(&conn, &wf.workflow.workflow_id).unwrap().unwrap();
        assert!(detail.flavors.is_empty());
    }

    #[test]
    fn delete_flavor_nonexistent_returns_false() {
        let conn = test_conn();
        assert!(!delete_flavor(&conn, "nope").unwrap());
    }

    // -----------------------------------------------------------------------
    // cascade from repo delete
    // -----------------------------------------------------------------------

    #[test]
    fn deleting_repo_cascades_to_workflows() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let wf = insert(&conn, &repo_id, "wf", "", &sample_steps()).unwrap();
        insert_flavor(&conn, &wf.workflow.workflow_id, "rust", None).unwrap();

        crate::db::repos::delete(&conn, &repo_id).unwrap();

        assert!(get_detail(&conn, &wf.workflow.workflow_id).unwrap().is_none());
        assert!(list_all(&conn).unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // step ordering
    // -----------------------------------------------------------------------

    #[test]
    fn steps_preserve_insertion_order() {
        let conn = test_conn();
        let repo_id = seed_repo(&conn);
        let steps = vec![
            CreateStepInput { name: "A".to_string(), prompt_template: "a".to_string() },
            CreateStepInput { name: "B".to_string(), prompt_template: "b".to_string() },
            CreateStepInput { name: "C".to_string(), prompt_template: "c".to_string() },
        ];
        let detail = insert(&conn, &repo_id, "wf", "", &steps).unwrap();

        assert_eq!(detail.steps[0].name, "A");
        assert_eq!(detail.steps[0].step_order, 0);
        assert_eq!(detail.steps[1].name, "B");
        assert_eq!(detail.steps[1].step_order, 1);
        assert_eq!(detail.steps[2].name, "C");
        assert_eq!(detail.steps[2].step_order, 2);
    }
}
