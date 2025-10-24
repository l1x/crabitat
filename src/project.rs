use serde::{Deserialize, Serialize};

use crate::agent::Agent;
use crate::model::Model;
use crate::tool::Tool;

/// Main project container for the Crabitat system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub models: Vec<Model>,
    #[serde(default)]
    pub agents: Vec<Agent>,
}

impl Project {
    /// Get a model by ID
    pub fn get_model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Get an agent by ID
    pub fn get_agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Get model assigned to an agent
    pub fn get_agent_model(&self, agent: &Agent) -> Option<&Model> {
        Some(&agent.model).and_then(|model_id| self.get_model(model_id))
    }

    /// Get all agents for a specific role
    pub fn get_agents_by_role(&self, role: &str) -> Vec<&Agent> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    /// Get available model names
    pub fn model_names(&self) -> Vec<String> {
        self.models.iter().map(|m| m.name.clone()).collect()
    }
}
