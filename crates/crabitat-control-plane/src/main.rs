mod db;
mod github;
mod handlers;
mod mission_service;
mod models;
mod workflow_registry;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::{delete, get, post};
use rusqlite::Connection;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "crabitat_control_plane=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let conn = db::init("crabitat.db");
    tracing::info!("database initialized");

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
            "/v1/workflows",
            get(handlers::workflows::list_all_workflows),
        )
        .route(
            "/v1/workflows/{name}",
            get(handlers::workflows::get_workflow),
        )
        .route(
            "/v1/workflows/{name}/flavors",
            post(handlers::workflows::create_flavor),
        )
        .route(
            "/v1/workflows/{name}/flavors/{flavor_id}",
            delete(handlers::workflows::delete_flavor).patch(handlers::workflows::update_flavor),
        )
        .route(
            "/v1/prompts/files",
            get(handlers::workflows::list_prompt_files),
        )
        .route(
            "/v1/prompts/content",
            post(handlers::workflows::get_prompts_content),
        )
        .route(
            "/v1/missions",
            post(handlers::missions::create_mission).get(handlers::missions::list_missions),
        )
        .route(
            "/v1/missions/{mission_id}",
            get(handlers::missions::get_mission),
        )
        .route(
            "/v1/tasks/next",
            get(handlers::tasks::get_next_task),
        )
        .route(
            "/v1/tasks/{task_id}/status",
            post(handlers::tasks::update_task_status),
        )
        .route("/v1/github/repos", get(handlers::github::search_repos))
        .route("/v1/settings", get(handlers::settings::list_settings))
        .route(
            "/v1/settings/{key}",
            get(handlers::settings::get_setting).post(handlers::settings::update_setting),
        )
        .route("/v1/system/status", get(handlers::system::get_status))
        .route("/v1/system/dirs", get(handlers::system::list_dirs))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();
    tracing::info!("listening on http://localhost:3001");
    axum::serve(listener, app).await.unwrap();
}
