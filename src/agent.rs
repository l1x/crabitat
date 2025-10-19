//! Agent types and configuration

use serde::{Deserialize, Serialize};

use crate::eid::ExternalId;

/// Represents an autonomous agent with specific capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: ExternalId,
    pub name: String,
    pub role: String,
    pub persona: String,
    pub model: ModelConfig,
    pub tools: Vec<String>, // Vec<Tool>
}

/// Configuration for the language model powering an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub temperature: f32,
    pub url: String,
}
