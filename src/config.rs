//! Simple TOML configuration loading with direct struct mapping.

use crate::agent::Agent;
use crate::error::ConfigError;
use crate::tool::Tool;
use serde::Deserialize;

use std::fs;

#[derive(Deserialize)]
struct Config {
    project: ProjectConfig,
    #[serde(rename = "agent")]
    agents: Vec<Agent>,
    #[serde(rename = "tool")]
    tools: Vec<Tool>,
}

#[derive(Deserialize)]
struct ProjectConfig {
    name: String,
    version: String,
}

#[derive(Deserialize)]
struct ToolConfig {
    allowed: bool,
}

/// Load configuration from TOML file
pub fn load_config(path: &str) -> Result<Vec<Agent>, ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead(e.to_string()))?;

    let config: Config =
        toml::from_str(&content).map_err(|e| ConfigError::TomlParse(e.to_string()))?;

    let agents: Result<Vec<_>, _> = config
        .agents
        .into_iter()
        .map(|agent_config| {
            let tools: Vec<String> = agent_config
                .tools
                .into_iter()
                .filter(|(_, tool)| tool.allowed)
                .map(|(name, _)| name)
                .collect();

            Ok(Agent {
                id: agent_config.id,
                name: agent_config.name,
                role: agent_config.role,
                persona: agent_config.persona,
                model: agent_config.model,
                tools,
            })
        })
        .collect();

    agents
}
