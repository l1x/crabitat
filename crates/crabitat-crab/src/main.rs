use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "The Crabitat Worker", long_about = None)]
struct Args {
    /// URL of the control-plane
    #[arg(short = 'u', long, default_value = "http://localhost:3001")]
    api_url: String,

    /// Polling interval in seconds
    #[arg(short = 'i', long, default_value_t = 10)]
    interval: u64,

    /// Agent name (e.g. "gemini", "claude")
    #[arg(long, default_value = "gemini")]
    agent: String,

    /// Optional root directory for cloning repos if no local_path is provided
    #[arg(long, default_value = "burrows")]
    burrows_root: String,

    /// Environment profile ('local', 'remote')
    #[arg(short = 'e', long, default_value = "local")]
    env: String,

    /// SSH Key name/path (Mock for AWS Secrets Manager integration)
    #[arg(long)]
    ssh_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskResponse {
    task: Task,
    git: GitInfo,
}

#[derive(Debug, Deserialize)]
struct Task {
    task_id: String,
    assembled_prompt: String,
    retry_count: i64,
    max_retries: i64,
}

#[derive(Debug, Deserialize)]
struct GitInfo {
    repo_url: String,
    branch: String,
    local_path: Option<String>,
}

#[derive(Serialize)]
struct UpdateStatusRequest {
    status: String,
}

#[derive(Serialize)]
struct CreateRunRequest {
    status: String,
    logs: Option<String>,
    summary: Option<String>,
    duration_ms: Option<i64>,
    tokens_used: Option<i64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "crabitat_crab=info".into()),
        )
        .init();
    let args = Args::parse();

    info!(
        "Crab worker started. API: {}, agent: {}, env: {}, interval: {}s",
        args.api_url, args.agent, args.env, args.interval
    );

    // Mock SSH Key Setup
    if let Some(key) = &args.ssh_key {
        info!("Setting up SSH key environment for: {}", key);
        // In a real AWS scenario, we would fetch from Secrets Manager here
    }

    let client = reqwest::Client::new();
    let worker_id = uuid::Uuid::new_v4().to_string();

    info!("Worker ID: {}", worker_id);

    loop {
        match poll_and_execute(&args, &client, &worker_id).await {
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

async fn get_env_path(
    client: &reqwest::Client,
    api_url: &str,
    env: &str,
    res_type: &str,
    res_name: &str,
) -> Option<String> {
    let url = format!(
        "{}/v1/system/env-path/{}/{}/{}",
        api_url, env, res_type, res_name
    );
    let res = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return None,
    };

    if res.status().is_success() {
        let data: serde_json::Value = res.json().await.ok()?;
        return data["path"].as_str().map(|s| s.to_string());
    }
    None
}

async fn poll_and_execute(
    args: &Args,
    client: &reqwest::Client,
    worker_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Fetch next task
    let res = client
        .get(format!("{}/v1/tasks/next", args.api_url))
        .query(&[("worker_id", worker_id)])
        .send()
        .await?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }

    let task_data: TaskResponse = res.json().await?;
    let task_id = &task_data.task.task_id;

    info!("Found task {} for repo {}", task_id, task_data.git.repo_url);

    // 2. Mark as running
    client
        .post(format!("{}/v1/tasks/{}/status", args.api_url, task_id))
        .json(&UpdateStatusRequest {
            status: "running".into(),
        })
        .send()
        .await?;

    // 3. Resolve Paths via API
    let agent_path = get_env_path(client, &args.api_url, &args.env, "agent", &args.agent)
        .await
        .unwrap_or_else(|| args.agent.clone());

    // 4. Setup Environment (Clone or CD)
    let repo_root = if let Some(lp) = &task_data.git.local_path {
        PathBuf::from(lp)
    } else {
        // Deterministic cache path based on repo URL
        let repo_name = task_data
            .git
            .repo_url
            .split('/')
            .next_back()
            .unwrap()
            .replace(".git", "");

        match get_env_path(client, &args.api_url, &args.env, "repo", &repo_name).await {
            Some(p) => PathBuf::from(p),
            None => {
                let cache_path = PathBuf::from(&args.burrows_root)
                    .join("cache")
                    .join(&repo_name);

                if !cache_path.exists() {
                    info!(
                        "Cloning repo {} to {:?}",
                        task_data.git.repo_url, cache_path
                    );
                    std::fs::create_dir_all(cache_path.parent().unwrap())?;
                    let status = Command::new("git")
                        .args([
                            "clone",
                            &task_data.git.repo_url,
                            cache_path.to_str().unwrap(),
                        ])
                        .status()?;
                    if !status.success() {
                        return Err("Failed to clone repository".into());
                    }
                }
                cache_path
            }
        }
    };

    // 5. Update repo state
    info!("Fetching latest state from origin...");
    let _ = Command::new("git")
        .arg("fetch")
        .arg("origin")
        .current_dir(&repo_root)
        .status();

    // 6. Create Worktree
    let worktree_name = task_data.git.branch.replace("/", "-");
    let worktree_path = repo_root.join("burrows").join(worktree_name);

    if worktree_path.exists() {
        info!("Cleaning up existing worktree {:?}", worktree_path);
        let _ = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(&repo_root)
            .status();
    }

    info!(
        "Creating worktree for branch {} at {:?}",
        task_data.git.branch, worktree_path
    );
    let status = Command::new("git")
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            &task_data.git.branch,
        ])
        .current_dir(&repo_root)
        .status()?;

    if !status.success() {
        info!(
            "Branch {} might already exist, attempting to track it...",
            task_data.git.branch
        );
        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                worktree_path.to_str().unwrap(),
                &task_data.git.branch,
            ])
            .current_dir(&repo_root)
            .status()?;
        if !status.success() {
            return Err("Failed to create worktree".into());
        }
    }

    // 7. Final Prompt Resolution
    let final_prompt = task_data
        .task
        .assembled_prompt
        .replace("{{worktree_path}}", worktree_path.to_str().unwrap());

    // 8. Execute Agent
    info!("Spawning agent: {} in {:?}", agent_path, worktree_path);
    let start_time = Instant::now();

    let mut child = Command::new(&agent_path);

    // Full tool use: ensure the agent inherits the parent shell's PATH and environment
    child.env("PATH", std::env::var("PATH").unwrap_or_default());

    // Agent-specific argument handling
    if args.agent == "claude" {
        child.args(["-p", &final_prompt]);
    } else if args.agent == "gemini" || args.agent == "gemini-cli" {
        // Use --yolo mode to allow the agent to execute tools (like gh) without confirmation
        child.args(["--approval-mode", "yolo", "-p", &final_prompt]);
    } else {
        child.arg(&final_prompt);
    }

    let output = child.current_dir(&worktree_path).output();

    let duration = start_time.elapsed();

    // 9. Handle Result
    let (success, logs) = match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined_logs = format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr);

            if out.status.success() {
                info!(
                    "Task {} completed successfully. Pushing changes...",
                    task_id
                );
                let _ = Command::new("git")
                    .args(["push", "origin", &task_data.git.branch])
                    .current_dir(&worktree_path)
                    .status();
                (true, combined_logs)
            } else {
                warn!(
                    "Task {} failed with exit code: {:?}",
                    task_id,
                    out.status.code()
                );
                (false, combined_logs)
            }
        }
        Err(e) => {
            error!("Failed to spawn agent: {}", e);
            (false, format!("Failed to spawn agent: {}", e))
        }
    };

    // 10. Record Run
    let final_status = if success { "completed" } else { "failed" };
    client
        .post(format!("{}/v1/tasks/{}/runs", args.api_url, task_id))
        .json(&CreateRunRequest {
            status: final_status.into(),
            logs: Some(logs),
            summary: None,
            duration_ms: Some(duration.as_millis() as i64),
            tokens_used: None,
        })
        .send()
        .await?;

    // 11. Report Result or Retry
    if success {
        client
            .post(format!("{}/v1/tasks/{}/status", args.api_url, task_id))
            .json(&UpdateStatusRequest {
                status: "completed".into(),
            })
            .send()
            .await?;
    } else if task_data.task.retry_count < task_data.task.max_retries {
        info!(
            "Retrying task {} ({} of {})",
            task_id,
            task_data.task.retry_count + 1,
            task_data.task.max_retries
        );
        client
            .post(format!("{}/v1/tasks/{}/retry", args.api_url, task_id))
            .send()
            .await?;
    } else {
        client
            .post(format!("{}/v1/tasks/{}/status", args.api_url, task_id))
            .json(&UpdateStatusRequest {
                status: "failed".into(),
            })
            .send()
            .await?;
    }

    Ok(true)
}
