//! Crabitat - Agent Workflow Orchestrator
//!
//! For coordinating conversations between multiple AI agents.
//! This system enables building sophisticated, autonomous workflows where specialized agents
//! collaborate to solve complex problems.

use env_logger;

mod agent;
mod config;
mod eid;
mod error;
mod model;
mod project;
mod task;
mod tool;

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting");
    let p = config::load_config("examples/project.toml");

    log::info!("{:?}", p);
}
