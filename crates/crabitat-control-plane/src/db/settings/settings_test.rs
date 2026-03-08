#[cfg(test)]
mod tests {
    use crate::db;
    use crate::db::settings;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        db::migrate(&conn);
        conn
    }

    #[test]
    fn set_and_get_setting() {
        let conn = test_conn();

        settings::set(&conn, "prompts_root", "/tmp/prompts").unwrap();
        let val = settings::get(&conn, "prompts_root").unwrap();

        assert_eq!(val, Some("/tmp/prompts".to_string()));
    }

    #[test]
    fn update_existing_setting() {
        let conn = test_conn();

        settings::set(&conn, "theme", "dark").unwrap();
        settings::set(&conn, "theme", "light").unwrap();

        let val = settings::get(&conn, "theme").unwrap();
        assert_eq!(val, Some("light".to_string()));
    }

    #[test]
    fn get_nonexistent_setting() {
        let conn = test_conn();
        let val = settings::get(&conn, "unknown").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn list_all_settings() {
        let conn = test_conn();

        settings::set(&conn, "s1", "v1").unwrap();
        settings::set(&conn, "s2", "v2").unwrap();

        let all = settings::list_all(&conn).unwrap();
        assert_eq!(all.len(), 2);

        let s1 = all.iter().find(|s| s.key == "s1").unwrap();
        assert_eq!(s1.value, "v1");
    }

    #[test]
    fn environment_paths() {
        let conn = test_conn();

        // Seed some paths
        conn.execute(
            "INSERT INTO environment_paths VALUES ('local', 'agent', 'gemini', '/path/to/gemini')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO environment_paths VALUES ('remote', 'agent', 'gemini', '/usr/bin/gemini')",
            [],
        )
        .unwrap();

        // Test get
        let local_path =
            settings::get_environment_path(&conn, "local", "agent", "gemini").unwrap();
        assert_eq!(local_path, Some("/path/to/gemini".to_string()));

        let remote_path =
            settings::get_environment_path(&conn, "remote", "agent", "gemini").unwrap();
        assert_eq!(remote_path, Some("/usr/bin/gemini".to_string()));

        let unknown = settings::get_environment_path(&conn, "local", "agent", "unknown").unwrap();
        assert_eq!(unknown, None);

        // Test list
        let all = settings::list_all_environment_paths(&conn).unwrap();
        assert_eq!(all.len(), 2);
    }
}
