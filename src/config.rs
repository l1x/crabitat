//! Simple TOML configuration loading with direct struct mapping.

use crate::agent::Agent;
use crate::error::ConfigError;
use crate::model::Model;
use crate::project::Project;
use crate::tool::Tool;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    // Project
    pub project: Project,
    // Tools
    #[serde(default)]
    pub tool: Vec<Tool>,
    // Models
    #[serde(default)]
    pub model: Vec<Model>,
    // Agents
    #[serde(default)]
    pub agent: Vec<Agent>,
}

/// Load configuration from TOML file
pub(crate) fn load_config(path: &str) -> Result<Project, ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead(e.to_string()))?;

    let project: Project =
        toml::from_str(&content).map_err(|e| ConfigError::TomlParse(e.to_string()))?;

    log::info!("Loaded project: {} v{}", project.name, project.version);
    log::info!("Models: {}", project.models.len());
    log::info!("Agents: {}", project.agents.len());
    log::info!("Tools: {}", project.tools.len());

    Ok(project)
}
