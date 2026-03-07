use clap::Parser;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, error, debug, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "The Crabitat Worker", long_about = None)]
struct Args {
    /// URL of the control-plane
    #[arg(short, long, default_value = "http://localhost:3001")]
    api_url: String,

    /// Polling interval in seconds
    #[arg(short, long, default_value_t = 10)]
    interval: u64,

    /// Agent command (e.g. "claude", "gemini-cli")
    #[arg(short, long, default_value = "claude")]
    agent: String,
}

#[derive(Debug, Deserialize)]
struct TaskResponse {
    task: Task,
    local_path: String,
}

#[derive(Debug, Deserialize)]
struct Task {
    task_id: String,
    assembled_prompt: String,
}

#[derive(Serialize)]
struct UpdateStatusRequest {
    status: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!("Crab worker started. API: {}, agent: {}, interval: {}s", 
          args.api_url, args.agent, args.interval);

    let client = reqwest::Client::new();

    loop {
        match poll_and_execute(&args, &client).await {
            Ok(executed) => {
                if !executed {
                    debug!("No tasks found, sleeping...");
                }
            }
            Err(e) => {
                error!("Worker error: {}", e);
            }
        }
        sleep(Duration::from_secs(args.interval)).await;
    }
}

async fn poll_and_execute(args: &Args, client: &reqwest::Client) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Fetch next task
    let res = client.get(format!("{}/v1/tasks/next", args.api_url)).send().await?;
    
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }

    let task_data: TaskResponse = res.json().await?;
    let task_id = &task_data.task.task_id;
    
    info!("Found task {} for repo {}", task_id, task_data.local_path);

    // 2. Mark as running
    client.post(format!("{}/v1/tasks/{}/status", args.api_url, task_id))
        .json(&UpdateStatusRequest { status: "running".into() })
        .send().await?;

    // 3. Execute Agent
    info!("Spawning agent: {} in {}", args.agent, task_data.local_path);
    
    let mut child = Command::new(&args.agent);
    
    // Configure arguments based on the agent's expected interface
    match args.agent.as_str() {
        "claude" => {
            child.args(["-p", &task_data.task.assembled_prompt]);
        }
        "gemini-cli" => {
            // Assuming gemini-cli takes the prompt as a direct argument
            child.arg(&task_data.task.assembled_prompt);
        }
        "codex" => {
            // Assuming codex takes the prompt as a direct argument
            child.arg(&task_data.task.assembled_prompt);
        }
        _ => {
            // Generic fallback
            child.arg(&task_data.task.assembled_prompt);
        }
    }

    child.current_dir(&task_data.local_path);

    let status = child.status();

    // 4. Report Result
    let final_status = match status {
        Ok(s) if s.success() => {
            info!("Task {} completed successfully", task_id);
            "completed"
        }
        Ok(s) => {
            warn!("Task {} failed with exit code: {:?}", task_id, s.code());
            "failed"
        }
        Err(e) => {
            error!("Failed to spawn agent: {}", e);
            "failed"
        }
    };

    client.post(format!("{}/v1/tasks/{}/status", args.api_url, task_id))
        .json(&UpdateStatusRequest { status: final_status.into() })
        .send().await?;

    Ok(true)
}
