use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::github;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn search_repos(
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<github::GhRepo>>, (StatusCode, Json<Value>)> {
    match github::search_repos(&params.q).await {
        Ok(repos) => Ok(Json(repos)),
        Err(e) => Err((StatusCode::BAD_GATEWAY, Json(json!({"error": e})))),
    }
}
