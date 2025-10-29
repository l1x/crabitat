//! Crabitat - Agent Workflow Orchestrator
//!
//! For coordinating conversations between multiple AI agents.
//! This system enables building sophisticated, autonomous workflows where specialized agents
//! collaborate to solve complex problems.

use env_logger;

use crate::error::CrabitatError;

mod agent;
mod config;
mod eid;
mod error;
mod model;
mod orchestrator;
mod project;
mod task;
mod tool;

#[tokio::main]
async fn main() -> Result<(), CrabitatError> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    log::info!("Starting");
    let project = config::load_config("examples/project.toml")?;
    log::info!("{:?}", project);

    // Iterate through all agents
    for agent in &project.agents {
        log::info!("Processing agent: {}", agent.name);

        if let Some(model) = project.get_agent_model(agent) {
            log::info!("Agent '{}' uses model '{}'", agent.name, model.name);

            // Test model show() method
            match model.show().await {
                Ok(details) => {
                    log::info!("Model details:\n{}", details);
                }
                Err(e) => {
                    log::error!("Failed to get model details for {}: {}", agent.name, e);
                }
            }
        } else {
            log::warn!("Agent '{}' has no model assigned", agent.name);
        }
    }

    for task in &project.tasks {
        log::info!("Task details:\n{}", task)
    }

    Ok(())
}
