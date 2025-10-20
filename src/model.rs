//! Model types and configuration

use serde::{Deserialize, Serialize};

/// Represents an autonomous agent with specific capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub temperature: f32,
    pub url: String,
}
