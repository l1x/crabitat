mod db;
mod github;
mod handlers;
mod mission_service;
mod models;
mod routes;
mod workflow_registry;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
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
                .unwrap_or_else(|_| "crabitat_control_plane=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let conn = db::init("crabitat.db");
    tracing::info!("database initialized");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = routes::create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();
    tracing::info!("listening on http://localhost:3001");
    axum::serve(listener, app).await.unwrap();
}
