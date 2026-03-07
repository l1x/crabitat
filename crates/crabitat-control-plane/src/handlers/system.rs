use crate::github;
use crate::models::system::SystemStatus;
use axum::Json;

pub async fn get_status() -> Json<SystemStatus> {
    let status = github::check_status().await;
    Json(status)
}
