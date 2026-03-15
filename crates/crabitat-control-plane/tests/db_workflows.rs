use crabitat_control_plane::db;
use crabitat_control_plane::db::workflows;
use rusqlite::Connection;

fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    db::migrate(&conn);
    conn
}

#[test]
fn insert_and_list_flavors() {
    let conn = test_conn();
    let f = workflows::insert_flavor(
        &conn,
        "my-workflow",
        "rust",
        &["rust/base.md".into(), "rust/extra.md".into()],
    )
    .unwrap();
    assert_eq!(f.workflow_name, "my-workflow");
    assert_eq!(f.name, "rust");
    assert_eq!(f.prompt_paths, vec!["rust/base.md", "rust/extra.md"]);

    let flavors = workflows::list_flavors_for_workflow(&conn, "my-workflow").unwrap();
    assert_eq!(flavors.len(), 1);
    assert_eq!(flavors[0].name, "rust");
}

#[test]
fn count_flavors() {
    let conn = test_conn();
    assert_eq!(
        workflows::count_flavors_for_workflow(&conn, "wf").unwrap(),
        0
    );
    workflows::insert_flavor(&conn, "wf", "a", &[]).unwrap();
    workflows::insert_flavor(&conn, "wf", "b", &["x.md".into()]).unwrap();
    assert_eq!(
        workflows::count_flavors_for_workflow(&conn, "wf").unwrap(),
        2
    );
}

#[test]
fn delete_existing_flavor() {
    let conn = test_conn();
    let f = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    assert!(workflows::delete_flavor(&conn, &f.flavor_id).unwrap());
    assert_eq!(
        workflows::list_flavors_for_workflow(&conn, "wf")
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn delete_nonexistent_flavor() {
    let conn = test_conn();
    assert!(!workflows::delete_flavor(&conn, "no-such-id").unwrap());
}

#[test]
fn duplicate_flavor_name_rejected() {
    let conn = test_conn();
    workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    let err = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap_err();
    assert!(err.contains("already exists"), "got: {err}");
}

#[test]
fn same_flavor_name_different_workflows() {
    let conn = test_conn();
    workflows::insert_flavor(&conn, "wf-a", "rust", &[]).unwrap();
    workflows::insert_flavor(&conn, "wf-b", "rust", &[]).unwrap();
    assert_eq!(
        workflows::count_flavors_for_workflow(&conn, "wf-a").unwrap(),
        1
    );
    assert_eq!(
        workflows::count_flavors_for_workflow(&conn, "wf-b").unwrap(),
        1
    );
}

#[test]
fn list_flavors_empty_workflow() {
    let conn = test_conn();
    let flavors = workflows::list_flavors_for_workflow(&conn, "nonexistent").unwrap();
    assert!(flavors.is_empty());
}

#[test]
fn flavor_prompt_paths_roundtrip() {
    let conn = test_conn();
    let paths = vec![
        "a/b.md".to_string(),
        "c/d.md".to_string(),
        "e.md".to_string(),
    ];
    let f = workflows::insert_flavor(&conn, "wf", "multi", &paths).unwrap();
    assert_eq!(f.prompt_paths, paths);

    let listed = workflows::list_flavors_for_workflow(&conn, "wf").unwrap();
    assert_eq!(listed[0].prompt_paths, paths);
}

#[test]
fn flavors_sorted_by_name() {
    let conn = test_conn();
    workflows::insert_flavor(&conn, "wf", "zulu", &[]).unwrap();
    workflows::insert_flavor(&conn, "wf", "alpha", &[]).unwrap();
    workflows::insert_flavor(&conn, "wf", "mike", &[]).unwrap();
    let flavors = workflows::list_flavors_for_workflow(&conn, "wf").unwrap();
    let names: Vec<&str> = flavors.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "mike", "zulu"]);
}

#[test]
fn re_add_soft_deleted_flavor() {
    let conn = test_conn();
    let f = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    workflows::delete_flavor(&conn, &f.flavor_id).unwrap();

    let result = workflows::insert_flavor(&conn, "wf", "rust", &[]);
    assert!(
        result.is_ok(),
        "Should be able to re-add soft-deleted flavor, but got: {:?}",
        result.err()
    );
}

#[test]
fn multiple_soft_delete_re_add_cycles() {
    let conn = test_conn();

    let f1 = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    workflows::delete_flavor(&conn, &f1.flavor_id).unwrap();

    let f2 = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    assert_ne!(f1.flavor_id, f2.flavor_id);
    workflows::delete_flavor(&conn, &f2.flavor_id).unwrap();

    let f3 = workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    assert_ne!(f2.flavor_id, f3.flavor_id);

    let flavors = workflows::list_flavors_for_workflow(&conn, "wf").unwrap();
    assert_eq!(flavors.len(), 1);
    assert_eq!(flavors[0].flavor_id, f3.flavor_id);
}

#[test]
fn active_duplicate_flavor_rejected() {
    let conn = test_conn();
    workflows::insert_flavor(&conn, "wf", "rust", &[]).unwrap();
    let result = workflows::insert_flavor(&conn, "wf", "rust", &[]);
    assert!(
        result.is_err(),
        "Should NOT be able to add duplicate active flavor"
    );
    let err = result.err().unwrap();
    assert!(err.contains("already exists"), "got: {err}");
}
