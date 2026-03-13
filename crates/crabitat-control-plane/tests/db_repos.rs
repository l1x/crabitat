use crabitat_control_plane::db;
use crabitat_control_plane::db::repos;
use rusqlite::Connection;

fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    db::migrate(&conn);
    conn
}

#[test]
fn insert_and_list_repos() {
    let conn = test_conn();
    repos::insert(&conn, "owner", "name", None, None).unwrap();
    let all = repos::list(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].owner, "owner");
    assert_eq!(all[0].name, "name");
}

#[test]
fn soft_delete_repo() {
    let conn = test_conn();
    let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();

    assert_eq!(repos::list(&conn).unwrap().len(), 1);
    assert!(repos::delete(&conn, &repo.repo_id).unwrap());
    assert_eq!(repos::list(&conn).unwrap().len(), 0);

    let deleted = repos::get_by_id(&conn, &repo.repo_id).unwrap().unwrap();
    assert!(deleted.deleted_at.is_some());
}

#[test]
fn delete_idempotency() {
    let conn = test_conn();
    let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();

    assert!(repos::delete(&conn, &repo.repo_id).unwrap());
    assert!(!repos::delete(&conn, &repo.repo_id).unwrap());
}

#[test]
fn get_nonexistent_repo() {
    let conn = test_conn();
    assert!(repos::get_by_id(&conn, "no-such-id").unwrap().is_none());
}

#[test]
fn re_add_soft_deleted_repo() {
    let conn = test_conn();
    let repo = repos::insert(&conn, "owner", "name", None, None).unwrap();
    repos::delete(&conn, &repo.repo_id).unwrap();

    let result = repos::insert(&conn, "owner", "name", None, None);
    assert!(
        result.is_ok(),
        "Should be able to re-add soft-deleted repo, but got: {:?}",
        result.err()
    );
}

#[test]
fn multiple_soft_delete_re_add_cycles() {
    let conn = test_conn();

    let repo1 = repos::insert(&conn, "owner", "name", None, None).unwrap();
    repos::delete(&conn, &repo1.repo_id).unwrap();

    let repo2 = repos::insert(&conn, "owner", "name", None, None).unwrap();
    assert_ne!(repo1.repo_id, repo2.repo_id);
    repos::delete(&conn, &repo2.repo_id).unwrap();

    let repo3 = repos::insert(&conn, "owner", "name", None, None).unwrap();
    assert_ne!(repo2.repo_id, repo3.repo_id);

    let all = repos::list(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].repo_id, repo3.repo_id);
}

#[test]
fn active_duplicate_repo_rejected() {
    let conn = test_conn();
    repos::insert(&conn, "owner", "name", None, None).unwrap();
    let result = repos::insert(&conn, "owner", "name", None, None);
    assert!(
        result.is_err(),
        "Should NOT be able to add duplicate active repo"
    );
    let err = result.err().unwrap();
    assert!(err.contains("UNIQUE constraint failed"), "got: {err}");
}
