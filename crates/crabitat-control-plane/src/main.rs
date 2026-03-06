mod db;
mod github;
mod handlers;
mod models;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::{delete, get, post};
use rusqlite::Connection;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

#[tokio::main]
async fn main() {
    let conn = db::init("crabitat.db");
    println!("database initialized");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = Router::new()
        .route(
            "/v1/repos",
            post(handlers::repos::create_repo).get(handlers::repos::list_repos),
        )
        .route(
            "/v1/repos/{repo_id}",
            get(handlers::repos::get_repo).delete(handlers::repos::delete_repo),
        )
        .route(
            "/v1/repos/{repo_id}/issues",
            get(handlers::issues::list_repo_issues),
        )
        .route(
            "/v1/repos/{repo_id}/issues/refresh",
            post(handlers::issues::refresh_repo_issues),
        )
        .route(
            "/v1/repos/{repo_id}/workflows",
            post(handlers::workflows::create_workflow)
                .get(handlers::workflows::list_repo_workflows),
        )
        .route(
            "/v1/workflows",
            get(handlers::workflows::list_all_workflows),
        )
        .route(
            "/v1/workflows/{workflow_id}",
            get(handlers::workflows::get_workflow)
                .put(handlers::workflows::update_workflow)
                .delete(handlers::workflows::delete_workflow),
        )
        .route(
            "/v1/workflows/{workflow_id}/flavors",
            post(handlers::workflows::create_flavor),
        )
        .route(
            "/v1/workflows/{workflow_id}/flavors/{flavor_id}",
            delete(handlers::workflows::delete_flavor),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();
    println!("listening on http://localhost:3001");
    axum::serve(listener, app).await.unwrap();
}
