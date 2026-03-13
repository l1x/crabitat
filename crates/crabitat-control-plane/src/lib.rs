pub mod db;
pub mod github;
pub mod handlers;
pub mod mission_service;
pub mod models;
pub mod routes;
pub mod workflow_registry;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}
