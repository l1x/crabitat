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
pub(crate) fn load_config(path: &str) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead(e.to_string()))?;

    let config: Config =
        toml::from_str(&content).map_err(|e| ConfigError::TomlParse(e.to_string()))?;

    // Project
    log::info!("project.name : {:?}", config.project.name);
    log::info!("project.version : {:?}", config.project.version);
    // Tools
    log::info!("tools : {:?}", config.tool);
    // Models
    log::info!("models : {:?}", config.model);
    // Agents
    log::info!("agents : {:?}", config.agent);

    Ok(config)
}
