use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "crabitat-crab", about = "Crab executor skeleton")]
struct Cli {
    #[arg(long)]
    crab_id: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Connect {
        #[arg(long, default_value = "http://127.0.0.1:8800")]
        control_plane: String,
    },
    Run {
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        burrow: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Connect { control_plane } => {
            info!(crab_id = %cli.crab_id, %control_plane, "connect skeleton");
        }
        Command::Run { task_id, burrow } => {
            info!(crab_id = %cli.crab_id, %task_id, %burrow, "run skeleton");
        }
    }

    Ok(())
}
