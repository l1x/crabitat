use anyhow::Result;
use clap::{Parser, Subcommand};
use crabitat_core::Mission;
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "crabitat-chief", about = "Chief runtime skeleton")]
struct Cli {
    #[arg(long, default_value = "chief-1")]
    chief_id: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    StartMission {
        #[arg(long)]
        prompt: String,
    },
    Watch,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::StartMission { prompt } => {
            let mission = Mission::new(prompt);
            info!(chief_id = %cli.chief_id, mission_id = %mission.id, "created mission skeleton");
        }
        Command::Watch => {
            info!(chief_id = %cli.chief_id, "watch mode skeleton started");
            tokio::signal::ctrl_c().await?;
            info!("watch mode stopping");
        }
    }

    Ok(())
}
