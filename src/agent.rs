//! Agent types and configuration

use serde::{Deserialize, Serialize};

/// Represents an autonomous agent with specific capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    //
    pub id: String,
    pub name: String,
    pub role: String,
    pub persona: String,
    //
}
