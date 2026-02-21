use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crabitat_core::{RunId, now_ms};
use crabitat_protocol::{Envelope, Heartbeat, MessageKind, TaskAssigned};
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, info, warn};
use uuid::Uuid;

const CRAB_PROMPT_TEMPLATE: &str = include_str!("crab_prompt.md");

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(
    name = "crabitat-crab",
    about = "Crab agent runtime — connects to control-plane and executes tasks"
)]
struct Cli {
    /// Control-plane base URL
    #[arg(long, global = true, default_value = "http://127.0.0.1:8800")]
    control_plane: String,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Register as a crab in a colony. Prints JSON with your crab_id.
    Register {
        #[arg(long)]
        colony_id: String,

        #[arg(long)]
        name: String,

        #[arg(long, default_value = "coder")]
        role: String,

        /// Explicit crab ID (auto-generated if omitted)
        #[arg(long)]
        crab_id: Option<String>,
    },

    /// Poll for tasks assigned to this crab. Prints task JSON or nothing.
    Poll {
        #[arg(long)]
        crab_id: String,
    },

    /// Start a run for a task. Prints JSON with run_id.
    StartRun {
        #[arg(long)]
        mission_id: String,

        #[arg(long)]
        task_id: String,

        #[arg(long)]
        crab_id: String,

        /// Working directory for this run (uses mission worktree if not specified)
        #[arg(long, default_value = ".")]
        burrow_path: String,
    },

    /// Complete a run (success or failure). Prints confirmation JSON.
    CompleteRun {
        #[arg(long)]
        run_id: String,

        /// "completed" or "failed"
        #[arg(long)]
        status: String,

        /// Brief summary of what happened
        #[arg(long)]
        summary: Option<String>,

        /// Structured result (e.g. "PASS" or "FAIL") for workflow condition evaluation
        #[arg(long)]
        result: Option<String>,

        /// End-to-end duration in milliseconds
        #[arg(long)]
        duration_ms: Option<u64>,
    },

    /// Print onboarding instructions for a Claude Code agent. Paste the output into a fresh session.
    Guide,

    /// Get full status snapshot from the control-plane.
    Status,

    /// List missions. Prints JSON array.
    Missions,

    /// List tasks. Prints JSON array.
    Tasks,

    /// Connect to a colony via WebSocket and auto-execute tasks (legacy mode)
    Connect {
        #[arg(long)]
        colony_id: String,

        #[arg(long)]
        name: String,

        #[arg(long, default_value = "coder")]
        role: String,

        #[arg(long, default_value = ".")]
        repo: PathBuf,

        #[arg(long)]
        crab_id: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct RegisterCrabBody {
    crab_id: String,
    colony_id: String,
    name: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct CrabResponse {
    crab_id: String,
}

#[derive(Debug, Serialize)]
struct StartRunBody {
    run_id: String,
    mission_id: String,
    task_id: String,
    crab_id: String,
    burrow_path: String,
    burrow_mode: String,
}

#[derive(Debug, Serialize)]
struct CompleteRunBody {
    run_id: String,
    status: String,
    summary: Option<String>,
    timing: Option<TimingBody>,
}

#[derive(Debug, Serialize)]
struct TimingBody {
    end_to_end_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TaskRecord {
    task_id: String,
    mission_id: String,
    title: String,
    assigned_crab_id: Option<String>,
    status: String,
    step_id: Option<String>,
    role: Option<String>,
    prompt: Option<String>,
    context: Option<String>,
    created_at_ms: u64,
    updated_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cp = &cli.control_plane;

    match cli.command {
        Cmd::Register { colony_id, name, role, crab_id } => {
            cmd_register(cp, &colony_id, &name, &role, crab_id).await?;
        }
        Cmd::Poll { crab_id } => {
            cmd_poll(cp, &crab_id).await?;
        }
        Cmd::StartRun { mission_id, task_id, crab_id, burrow_path } => {
            cmd_start_run(cp, &mission_id, &task_id, &crab_id, &burrow_path).await?;
        }
        Cmd::CompleteRun { run_id, status, summary, result, duration_ms } => {
            cmd_complete_run(cp, &run_id, &status, summary, result, duration_ms).await?;
        }
        Cmd::Guide => {
            cmd_guide(cp);
            return Ok(());
        }
        Cmd::Status => {
            cmd_status(cp).await?;
        }
        Cmd::Missions => {
            cmd_missions(cp).await?;
        }
        Cmd::Tasks => {
            cmd_tasks(cp).await?;
        }
        Cmd::Connect { colony_id, name, role, repo, crab_id } => {
            run_connect(cp, &colony_id, &name, &role, &repo, crab_id).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

async fn cmd_register(
    cp: &str,
    colony_id: &str,
    name: &str,
    role: &str,
    crab_id: Option<String>,
) -> Result<()> {
    let http = Client::new();
    let crab_id = crab_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let resp = http
        .post(format!("{cp}/v1/crabs/register"))
        .json(&RegisterCrabBody {
            crab_id,
            colony_id: colony_id.to_string(),
            name: name.to_string(),
            role: role.to_string(),
        })
        .send()
        .await
        .context("failed to reach control-plane")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("registration failed (HTTP {status}): {body}");
    }
    println!("{body}");
    Ok(())
}

fn cmd_guide(cp: &str) {
    print!(
        r#"You are a crab agent in a Crabitat colony. You interact with the control-plane using the `crabitat-crab` CLI and execute tasks yourself using your own tools (Read, Write, Edit, Bash, Glob, Grep).

## Step 1: Register

Pick a name and role for yourself, then register:

```
crabitat-crab register --control-plane {cp} --colony-id <COLONY_ID> --name <NAME> --role <ROLE>
```

This prints JSON with your `crab_id`. Save it — you need it for all subsequent commands.

To find available colonies, run: `crabitat-crab status --control-plane {cp}`

## Step 2: Poll for tasks

```
crabitat-crab poll --control-plane {cp} --crab-id <YOUR_CRAB_ID>
```

If a task is assigned to you, this prints the task JSON (with `task_id`, `mission_id`, `title`). If nothing is pending, it prints nothing — wait 5 seconds and poll again.

## Step 3: Get mission context

```
crabitat-crab missions --control-plane {cp}
```

Find the mission matching your task's `mission_id` and read its `prompt` field for context.

## Step 4: Start the run

```
crabitat-crab start-run --control-plane {cp} --mission-id <MISSION_ID> --task-id <TASK_ID> --crab-id <YOUR_CRAB_ID>
```

This prints JSON with the `run_id`. Save it.

## Step 5: Do the work

Execute the task using your own tools. The task title and mission prompt tell you what to do.

## Step 6: Complete the run

```
crabitat-crab complete-run --control-plane {cp} --run-id <RUN_ID> --status completed --summary "Brief description of what you did"
```

Use `--status failed` if the task could not be completed, with the error in `--summary`.

## Step 7: Loop

Go back to Step 2 and poll for the next task. Never stop unless told to shut down.

## Other commands

- `crabitat-crab status --control-plane {cp}` — full snapshot
- `crabitat-crab tasks --control-plane {cp}` — list all tasks
- `crabitat-crab missions --control-plane {cp}` — list all missions

## Rules

- Always call `complete-run`, whether you succeed or fail
- Keep summaries under 4 KiB
- If a task is ambiguous, do your best and note assumptions in the summary
"#
    );
}

async fn cmd_poll(cp: &str, crab_id: &str) -> Result<()> {
    let http = Client::new();
    let resp =
        http.get(format!("{cp}/v1/tasks")).send().await.context("failed to reach control-plane")?;

    let tasks: Vec<TaskRecord> = resp.json().await.context("bad response")?;

    // Find tasks assigned to this crab (queued or assigned status)
    let pending: Vec<&TaskRecord> = tasks
        .iter()
        .filter(|t| {
            t.assigned_crab_id.as_deref() == Some(crab_id)
                && (t.status == "queued" || t.status == "assigned")
        })
        .collect();

    if pending.is_empty() {
        return Ok(());
    }

    let json = serde_json::to_string_pretty(&pending[0])?;
    println!("{json}");
    Ok(())
}

async fn cmd_start_run(
    cp: &str,
    mission_id: &str,
    task_id: &str,
    crab_id: &str,
    burrow_path: &str,
) -> Result<()> {
    let http = Client::new();
    let run_id = RunId::new().to_string();

    let resp = http
        .post(format!("{cp}/v1/runs/start"))
        .json(&StartRunBody {
            run_id,
            mission_id: mission_id.to_string(),
            task_id: task_id.to_string(),
            crab_id: crab_id.to_string(),
            burrow_path: burrow_path.to_string(),
            burrow_mode: "worktree".to_string(),
        })
        .send()
        .await
        .context("failed to reach control-plane")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("start-run failed (HTTP {status}): {body}");
    }
    println!("{body}");
    Ok(())
}

async fn cmd_complete_run(
    cp: &str,
    run_id: &str,
    status: &str,
    summary: Option<String>,
    result: Option<String>,
    duration_ms: Option<u64>,
) -> Result<()> {
    let http = Client::new();

    // If a result is provided, wrap it in a JSON summary
    let final_summary = match (summary, result) {
        (Some(sum), Some(res)) => {
            Some(serde_json::json!({"summary": sum, "result": res}).to_string())
        }
        (Some(sum), None) => Some(sum),
        (None, Some(res)) => Some(serde_json::json!({"result": res}).to_string()),
        (None, None) => None,
    };

    let resp = http
        .post(format!("{cp}/v1/runs/complete"))
        .json(&CompleteRunBody {
            run_id: run_id.to_string(),
            status: status.to_string(),
            summary: final_summary,
            timing: duration_ms.map(|ms| TimingBody { end_to_end_ms: Some(ms) }),
        })
        .send()
        .await
        .context("failed to reach control-plane")?;

    let status_code = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status_code.is_success() {
        anyhow::bail!("complete-run failed (HTTP {status_code}): {body}");
    }
    println!("{body}");
    Ok(())
}

async fn cmd_status(cp: &str) -> Result<()> {
    let http = Client::new();
    let resp = http
        .get(format!("{cp}/v1/status"))
        .send()
        .await
        .context("failed to reach control-plane")?;

    let body = resp.text().await.unwrap_or_default();
    // Pretty-print the JSON
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{body}");
    }
    Ok(())
}

async fn cmd_missions(cp: &str) -> Result<()> {
    let http = Client::new();
    let resp = http
        .get(format!("{cp}/v1/missions"))
        .send()
        .await
        .context("failed to reach control-plane")?;

    let body = resp.text().await.unwrap_or_default();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{body}");
    }
    Ok(())
}

async fn cmd_tasks(cp: &str) -> Result<()> {
    let http = Client::new();
    let resp =
        http.get(format!("{cp}/v1/tasks")).send().await.context("failed to reach control-plane")?;

    let body = resp.text().await.unwrap_or_default();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{body}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Legacy WebSocket connect flow
// ---------------------------------------------------------------------------

async fn run_connect(
    control_plane: &str,
    colony_id: &str,
    name: &str,
    role: &str,
    repo: &Path,
    crab_id_opt: Option<String>,
) -> Result<()> {
    let http = Client::new();
    let crab_id = crab_id_opt.unwrap_or_else(|| Uuid::new_v4().to_string());

    info!(crab_id = %crab_id, name = %name, role = %role, "registering with control-plane");

    let resp = http
        .post(format!("{control_plane}/v1/crabs/register"))
        .json(&RegisterCrabBody {
            crab_id: crab_id.clone(),
            colony_id: colony_id.to_string(),
            name: name.to_string(),
            role: role.to_string(),
        })
        .send()
        .await
        .context("failed to register crab")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("registration failed: {body}");
    }

    let crab_resp: CrabResponse = resp.json().await.context("bad registration response")?;
    info!(crab_id = %crab_resp.crab_id, "registered successfully");

    let ws_url = format!(
        "{}/v1/ws/crab/{}",
        control_plane.replacen("http://", "ws://", 1).replacen("https://", "wss://", 1),
        crab_id
    );
    info!(url = %ws_url, "connecting WebSocket");

    let (ws_stream, _) = connect_async(&ws_url).await.context("WebSocket connect failed")?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    info!("WebSocket connected — listening for tasks");

    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let envelope = Envelope::new(
                    &crab_id,
                    "control-plane",
                    MessageKind::Heartbeat(Heartbeat {
                        crab_id: crab_id.clone(),
                        healthy: true,
                    }),
                    now_ms(),
                );
                if let Ok(json) = serde_json::to_string(&envelope)
                    && ws_write.send(WsMessage::Text(json)).await.is_err()
                {
                    warn!("heartbeat send failed, reconnecting");
                    break;
                }
            }
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        match serde_json::from_str::<Envelope>(&text) {
                            Ok(envelope) => {
                                if let MessageKind::TaskAssigned(task) = envelope.kind {
                                    info!(
                                        task_id = %task.task_id,
                                        title = %task.title,
                                        "task assigned"
                                    );
                                    if let Err(e) = handle_task(
                                        &http,
                                        control_plane,
                                        &crab_id,
                                        name,
                                        role,
                                        colony_id,
                                        repo,
                                        &task,
                                    ).await {
                                        error!(err = %e, "task execution failed");
                                    }
                                }
                            }
                            Err(e) => warn!(err = %e, "ignoring unparseable WS message"),
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        info!("WebSocket closed by server");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(err = %e, "WebSocket error");
                        break;
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down");
                break;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Legacy task execution (used by `connect` subcommand)
// ---------------------------------------------------------------------------

async fn handle_task(
    http: &Client,
    control_plane: &str,
    crab_id: &str,
    crab_name: &str,
    crab_role: &str,
    colony_name: &str,
    repo: &Path,
    task: &TaskAssigned,
) -> Result<()> {
    let task_id_str = task.task_id.to_string();
    let mission_id_str = task.mission_id.to_string();
    let short_id = &task_id_str[..8.min(task_id_str.len())];
    let burrow_dir = repo.join("burrows").join(short_id);

    let run_id = RunId::new().to_string();
    let started_at = now_ms();

    let start_resp = http
        .post(format!("{control_plane}/v1/runs/start"))
        .json(&StartRunBody {
            run_id: run_id.clone(),
            mission_id: mission_id_str.clone(),
            task_id: task_id_str.clone(),
            crab_id: crab_id.to_string(),
            burrow_path: burrow_dir.to_string_lossy().to_string(),
            burrow_mode: "worktree".to_string(),
        })
        .send()
        .await;

    let run_registered = match start_resp {
        Ok(r) if r.status().is_success() => {
            info!(run_id = %run_id, "run started");
            true
        }
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            warn!(run_id = %run_id, body = %body, "start_run returned error");
            false
        }
        Err(e) => {
            warn!(run_id = %run_id, err = %e, "start_run request failed");
            false
        }
    };

    let result =
        execute_in_burrow(crab_name, crab_role, colony_name, repo, task, &burrow_dir).await;

    let end_to_end_ms = now_ms().saturating_sub(started_at);

    let (status, summary) = match &result {
        Ok(output) => {
            let status = if output.success { "completed" } else { "failed" };
            (status, output.summary.clone())
        }
        Err(e) => ("failed", format!("task setup failed: {e}")),
    };

    info!(status = status, "task finished");

    if run_registered {
        let complete_resp = http
            .post(format!("{control_plane}/v1/runs/complete"))
            .json(&CompleteRunBody {
                run_id: run_id.clone(),
                status: status.to_string(),
                summary: Some(summary),
                timing: Some(TimingBody { end_to_end_ms: Some(end_to_end_ms) }),
            })
            .send()
            .await;

        match complete_resp {
            Ok(r) if r.status().is_success() => info!(run_id = %run_id, "run completed"),
            Ok(r) => {
                let body = r.text().await.unwrap_or_default();
                warn!(run_id = %run_id, body = %body, "complete_run returned error");
            }
            Err(e) => warn!(run_id = %run_id, err = %e, "complete_run request failed"),
        }
    }

    if burrow_dir.exists() {
        let cleanup = TokioCommand::new("git")
            .args([
                "-C",
                &repo.to_string_lossy(),
                "worktree",
                "remove",
                "--force",
                &burrow_dir.to_string_lossy(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match cleanup {
            Ok(s) if s.success() => info!("worktree cleaned up"),
            _ => warn!(
                burrow = %burrow_dir.display(),
                "worktree cleanup failed (manual removal needed)"
            ),
        }
    }

    result.map(|_| ())
}

struct TaskOutput {
    success: bool,
    summary: String,
}

async fn execute_in_burrow(
    crab_name: &str,
    crab_role: &str,
    colony_name: &str,
    repo: &Path,
    task: &TaskAssigned,
    burrow_dir: &Path,
) -> Result<TaskOutput> {
    info!(burrow = %burrow_dir.display(), "creating worktree");
    let worktree_result = TokioCommand::new("git")
        .args([
            "-C",
            &repo.to_string_lossy(),
            "worktree",
            "add",
            &burrow_dir.to_string_lossy(),
            "HEAD",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to spawn git worktree")?;

    if !worktree_result.status.success() {
        let stderr = String::from_utf8_lossy(&worktree_result.stderr);
        anyhow::bail!("git worktree add failed: {stderr}");
    }
    info!("worktree created");

    let prompt_content = CRAB_PROMPT_TEMPLATE
        .replace("{{crab_name}}", crab_name)
        .replace("{{crab_role}}", crab_role)
        .replace("{{colony_name}}", colony_name)
        .replace("{{task_title}}", &task.title)
        .replace("{{mission_prompt}}", &task.mission_prompt);

    let claude_md_path = burrow_dir.join("CLAUDE.md");
    std::fs::write(&claude_md_path, &prompt_content)
        .context("failed to write CLAUDE.md into burrow")?;
    info!(path = %claude_md_path.display(), "wrote CLAUDE.md");

    info!(burrow = %burrow_dir.display(), "spawning claude");

    let claude_output = TokioCommand::new("claude")
        .current_dir(burrow_dir)
        .env_remove("CLAUDECODE")
        .arg("-p")
        .arg(&task.title)
        .arg("--output-format")
        .arg("text")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let (success, stdout, stderr) = match claude_output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            (output.status.success(), stdout, stderr)
        }
        Err(e) => {
            error!(err = %e, "failed to spawn claude");
            (false, String::new(), format!("spawn error: {e}"))
        }
    };

    let summary = if stdout.is_empty() {
        if stderr.is_empty() { "(no output)".to_string() } else { stderr }
    } else {
        let max = 4096;
        if stdout.len() > max { format!("{}... [truncated]", &stdout[..max]) } else { stdout }
    };

    Ok(TaskOutput { success, summary })
}
