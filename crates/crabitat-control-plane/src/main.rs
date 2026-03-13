use std::sync::{Arc, Mutex};

use crabitat_control_plane::{AppState, db, routes};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "crabitat_control_plane=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "crabitat.db".into());
    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".into());

    let conn = db::init(&db_path);
    tracing::info!("database initialized at {}", db_path);

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = routes::create_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}
