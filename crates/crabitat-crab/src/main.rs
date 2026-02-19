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
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Connect to a colony and listen for task assignments
    Connect {
        /// Control-plane base URL
        #[arg(long, default_value = "http://127.0.0.1:8800")]
        control_plane: String,

        /// Colony to join
        #[arg(long)]
        colony_id: String,

        /// Display name for this crab
        #[arg(long)]
        name: String,

        /// Role within the colony (e.g. coder, reviewer, architect)
        #[arg(long, default_value = "coder")]
        role: String,

        /// Git repository root (used for creating worktrees)
        #[arg(long, default_value = ".")]
        repo: PathBuf,

        /// Explicit crab ID (auto-generated if omitted)
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

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match Cli::parse().command {
        Cmd::Connect { control_plane, colony_id, name, role, repo, crab_id } => {
            run_connect(&control_plane, &colony_id, &name, &role, &repo, crab_id).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Connect flow
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

    // 1. Register with control-plane
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

    // 2. Connect WebSocket
    let ws_url = format!(
        "{}/v1/ws/crab/{}",
        control_plane.replacen("http://", "ws://", 1).replacen("https://", "wss://", 1),
        crab_id
    );
    info!(url = %ws_url, "connecting WebSocket");

    let (ws_stream, _) = connect_async(&ws_url).await.context("WebSocket connect failed")?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    info!("WebSocket connected — listening for tasks");

    // 3. Event loop: heartbeat + task dispatch
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
// Task execution
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

    // 1. Create git worktree
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
        .status()
        .await;

    match worktree_result {
        Ok(status) if status.success() => {
            info!("worktree created");
        }
        Ok(status) => {
            warn!(code = ?status.code(), "worktree creation exited non-zero, attempting to continue");
        }
        Err(e) => {
            error!(err = %e, "failed to create worktree");
            return Err(e.into());
        }
    }

    // 2. Write CLAUDE.md (system prompt for Claude Code)
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

    // 3. Start run via control-plane
    let run_id = RunId::new().to_string();
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

    match start_resp {
        Ok(r) if r.status().is_success() => info!(run_id = %run_id, "run started"),
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            warn!(run_id = %run_id, body = %body, "start_run returned error");
        }
        Err(e) => warn!(run_id = %run_id, err = %e, "start_run request failed"),
    }

    // 4. Spawn claude CLI
    let started_at = now_ms();
    info!(burrow = %burrow_dir.display(), "spawning claude");

    let claude_output = TokioCommand::new("claude")
        .current_dir(&burrow_dir)
        .arg("-p")
        .arg(&task.title)
        .arg("--output-format")
        .arg("text")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let (exit_status, stdout, stderr) = match claude_output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            (if output.status.success() { "completed" } else { "failed" }, stdout, stderr)
        }
        Err(e) => {
            error!(err = %e, "failed to spawn claude");
            ("failed", String::new(), format!("spawn error: {e}"))
        }
    };

    let end_to_end_ms = now_ms().saturating_sub(started_at);

    // 5. Summarise output (truncate to 4 KiB for the API)
    let summary = if stdout.is_empty() {
        if stderr.is_empty() { "(no output)".to_string() } else { stderr.clone() }
    } else {
        let max = 4096;
        if stdout.len() > max {
            format!("{}... [truncated]", &stdout[..max])
        } else {
            stdout.clone()
        }
    };

    info!(status = exit_status, bytes = summary.len(), "claude finished");

    // 6. Complete run
    let complete_resp = http
        .post(format!("{control_plane}/v1/runs/complete"))
        .json(&CompleteRunBody {
            run_id: run_id.clone(),
            status: exit_status.to_string(),
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

    // 7. Clean up worktree
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
        _ => {
            warn!(burrow = %burrow_dir.display(), "worktree cleanup failed (manual removal needed)")
        }
    }

    Ok(())
}
