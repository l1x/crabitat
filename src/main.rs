//! Crabitat - Agent Workflow Orchestrator
//!
//! For coordinating conversations between multiple AI agents.
//! This system enables building sophisticated, autonomous workflows where specialized agents
//! collaborate to solve complex problems.

use env_logger;
use log::info;

mod agent;
mod config;
mod eid;
mod error;
mod project;
mod task;
mod tool;

fn main() {
    log::info!("Starting...")
    let p = config::load_config();
}
