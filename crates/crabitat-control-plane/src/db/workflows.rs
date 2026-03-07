use crate::models::workflows::WorkflowFlavor;
use rusqlite::{Connection, params};

pub fn list_flavors_for_workflow(
    conn: &Connection,
    workflow_name: &str,
) -> Result<Vec<WorkflowFlavor>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT flavor_id, workflow_name, name, prompt_paths
             FROM workflow_flavors WHERE workflow_name = ?1 ORDER BY name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![workflow_name], |row| {
            let paths_json: String = row.get(3)?;
            let prompt_paths: Vec<String> = serde_json::from_str(&paths_json).unwrap_or_default();
            Ok(WorkflowFlavor {
                flavor_id: row.get(0)?,
                workflow_name: row.get(1)?,
                name: row.get(2)?,
                prompt_paths,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

pub fn count_flavors_for_workflow(conn: &Connection, workflow_name: &str) -> Result<usize, String> {
    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_flavors WHERE workflow_name = ?",
            params![workflow_name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count)
}

pub fn insert_flavor(
    conn: &Connection,
    workflow_name: &str,
    name: &str,
    prompt_paths: &[String],
) -> Result<WorkflowFlavor, String> {
    let flavor_id = uuid::Uuid::new_v4().to_string();
    let prompt_paths_json = serde_json::to_string(prompt_paths).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO workflow_flavors (flavor_id, workflow_name, name, prompt_paths) VALUES (?1, ?2, ?3, ?4)",
        params![flavor_id, workflow_name, name, prompt_paths_json],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            format!("A flavor named '{}' already exists for this workflow.", name)
        } else {
            e.to_string()
        }
    })?;

    Ok(WorkflowFlavor {
        flavor_id,
        workflow_name: workflow_name.to_string(),
        name: name.to_string(),
        prompt_paths: prompt_paths.to_vec(),
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

pub fn update_flavor(
    conn: &Connection,
    flavor_id: &str,
    name: &str,
    prompt_paths: &[String],
) -> Result<(), String> {
    let prompt_paths_json = serde_json::to_string(prompt_paths).map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE workflow_flavors SET name = ?1, prompt_paths = ?2 WHERE flavor_id = ?3",
        params![name, prompt_paths_json, flavor_id],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            format!("A flavor named '{}' already exists for this workflow.", name)
        } else {
            e.to_string()
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        db::migrate(&conn);
        conn
    }

    #[test]
    fn insert_and_list_flavors() {
        let conn = setup();
        let f = insert_flavor(
            &conn,
            "my-workflow",
            "rust",
            &["rust/base.md".into(), "rust/extra.md".into()],
        )
        .unwrap();
        assert_eq!(f.workflow_name, "my-workflow");
        assert_eq!(f.name, "rust");
        assert_eq!(f.prompt_paths, vec!["rust/base.md", "rust/extra.md"]);

        let flavors = list_flavors_for_workflow(&conn, "my-workflow").unwrap();
        assert_eq!(flavors.len(), 1);
        assert_eq!(flavors[0].name, "rust");
    }

    #[test]
    fn count_flavors() {
        let conn = setup();
        assert_eq!(count_flavors_for_workflow(&conn, "wf").unwrap(), 0);
        insert_flavor(&conn, "wf", "a", &[]).unwrap();
        insert_flavor(&conn, "wf", "b", &["x.md".into()]).unwrap();
        assert_eq!(count_flavors_for_workflow(&conn, "wf").unwrap(), 2);
    }

    #[test]
    fn delete_existing_flavor() {
        let conn = setup();
        let f = insert_flavor(&conn, "wf", "rust", &[]).unwrap();
        assert!(delete_flavor(&conn, &f.flavor_id).unwrap());
        assert_eq!(list_flavors_for_workflow(&conn, "wf").unwrap().len(), 0);
    }

    #[test]
    fn delete_nonexistent_flavor() {
        let conn = setup();
        assert!(!delete_flavor(&conn, "no-such-id").unwrap());
    }

    #[test]
    fn duplicate_flavor_name_rejected() {
        let conn = setup();
        insert_flavor(&conn, "wf", "rust", &[]).unwrap();
        let err = insert_flavor(&conn, "wf", "rust", &[]).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn same_flavor_name_different_workflows() {
        let conn = setup();
        insert_flavor(&conn, "wf-a", "rust", &[]).unwrap();
        insert_flavor(&conn, "wf-b", "rust", &[]).unwrap();
        assert_eq!(count_flavors_for_workflow(&conn, "wf-a").unwrap(), 1);
        assert_eq!(count_flavors_for_workflow(&conn, "wf-b").unwrap(), 1);
    }

    #[test]
    fn list_flavors_empty_workflow() {
        let conn = setup();
        let flavors = list_flavors_for_workflow(&conn, "nonexistent").unwrap();
        assert!(flavors.is_empty());
    }

    #[test]
    fn flavor_prompt_paths_roundtrip() {
        let conn = setup();
        let paths = vec![
            "a/b.md".to_string(),
            "c/d.md".to_string(),
            "e.md".to_string(),
        ];
        let f = insert_flavor(&conn, "wf", "multi", &paths).unwrap();
        assert_eq!(f.prompt_paths, paths);

        let listed = list_flavors_for_workflow(&conn, "wf").unwrap();
        assert_eq!(listed[0].prompt_paths, paths);
    }

    #[test]
    fn flavors_sorted_by_name() {
        let conn = setup();
        insert_flavor(&conn, "wf", "zulu", &[]).unwrap();
        insert_flavor(&conn, "wf", "alpha", &[]).unwrap();
        insert_flavor(&conn, "wf", "mike", &[]).unwrap();
        let flavors = list_flavors_for_workflow(&conn, "wf").unwrap();
        let names: Vec<&str> = flavors.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mike", "zulu"]);
    }
}
