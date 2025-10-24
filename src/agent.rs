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
    /// Model ID this agent should use
    pub model: String,
    /// Tool IDs this agent has access to
    #[serde(default)]
    pub tools: Vec<String>,
}

impl Agent {
    /// Check if agent can use a specific tool
    pub fn has_tool(&self, tool_id: &str) -> bool {
        self.tools.contains(&tool_id.to_string())
    }

    /// Get assigned model name (if available)
    pub fn model_name(&self) -> &str {
        &self.model // Just return reference since it's required String now
    }
}
