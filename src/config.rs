//! Simple TOML configuration loading with direct struct mapping.

use crate::error::ConfigError;
use crate::model::Model;
use crate::project::Project;
use crate::tool::Tool;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub project: Project,
    #[serde(default)]
    pub tool: Vec<Tool>,
    #[serde(default)]
    pub model: Vec<Model>,
}

/// Load configuration from TOML file
pub(crate) fn load_config(path: &str) -> Result<(), ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead(e.to_string()))?;

    let config: Config =
        toml::from_str(&content).map_err(|e| ConfigError::TomlParse(e.to_string()))?;

    log::info!("project.name : {:?}", config.project.name);
    log::info!("project.version : {:?}", config.project.version);
    log::info!("tools : {:?}", config.tool);
    log::info!("models : {:?}", config.model);

    Ok(())
}
