use crate::models::settings::Setting;
use crate::models::system::EnvironmentPath;
use rusqlite::{Connection, Result};

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?")?;
    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn get_environment_path(
    conn: &Connection,
    env: &str,
    res_type: &str,
    res_name: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT path FROM environment_paths 
         WHERE environment = ? AND resource_type = ? AND resource_name = ?",
    )?;
    let mut rows = stmt.query([env, res_type, res_name])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn list_all_environment_paths(conn: &Connection) -> Result<Vec<EnvironmentPath>> {
    let mut stmt = conn
        .prepare("SELECT environment, resource_type, resource_name, path, created_at, updated_at FROM environment_paths")?;
    let rows = stmt.query_map([], |row| {
        Ok(EnvironmentPath {
            environment: row.get(0)?,
            resource_type: row.get(1)?,
            resource_name: row.get(2)?,
            path: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    let mut paths = Vec::new();
    for path in rows {
        paths.push(path?);
    }
    Ok(paths)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [key, value],
    )?;
    Ok(())
}

pub fn get_full(conn: &Connection, key: &str) -> Result<Option<Setting>> {
    let mut stmt =
        conn.prepare("SELECT key, value, created_at, updated_at FROM settings WHERE key = ?")?;
    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Setting {
            key: row.get(0)?,
            value: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_all(conn: &Connection) -> Result<Vec<Setting>> {
    let mut stmt = conn.prepare("SELECT key, value, created_at, updated_at FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok(Setting {
            key: row.get(0)?,
            value: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })?;

    let mut settings = Vec::new();
    for setting in rows {
        settings.push(setting?);
    }
    Ok(settings)
}

#[cfg(test)]
mod settings_test;
