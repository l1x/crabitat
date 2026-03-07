use crate::github;
use crate::models::system::SystemStatus;
use axum::extract::Query;
use axum::Json;
use serde::Deserialize;
use std::fs;

pub async fn get_status() -> Json<SystemStatus> {
    let status = github::check_status().await;
    Json(status)
}

#[derive(Deserialize)]
pub struct DirQuery {
    pub q: String,
}

pub async fn list_dirs(Query(params): Query<DirQuery>) -> Json<Vec<String>> {
    let query = params.q;
    if query.is_empty() {
        return Json(vec![]);
    }

    let path = std::path::Path::new(&query);

    // Determine the directory to search in and the prefix to match
    let (search_dir, prefix) = if path.is_dir() && query.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new("/")),
            path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        )
    };

    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.to_lowercase().starts_with(&prefix.to_lowercase())
                        && !name.starts_with('.')
                    {
                        let full_path = entry.path().to_string_lossy().to_string();
                        dirs.push(full_path);
                    }
                }
            }
            if dirs.len() >= 10 {
                break;
            } // Limit results
        }
    }

    Json(dirs)
}
