use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crabitat_core::RunId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(
    name = "crabitat-crab",
    about = "Crab agent runtime — interacts with control-plane via REST"
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
    /// Register a crab with a repo. Prints JSON with your crab_id.
    Register {
        #[arg(long)]
        repo_id: String,

        #[arg(long)]
        name: String,

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

        /// Number of prompt/input tokens consumed
        #[arg(long)]
        prompt_tokens: Option<u32>,

        /// Number of completion/output tokens generated
        #[arg(long)]
        completion_tokens: Option<u32>,

        /// Total tokens (prompt + completion). Auto-computed if omitted.
        #[arg(long)]
        total_tokens: Option<u32>,
    },

    /// Print onboarding instructions for a Claude Code agent. Paste the output into a fresh session.
    Guide,

    /// Get full status snapshot from the control-plane.
    Status,

    /// List missions. Prints JSON array.
    Missions,

    /// List tasks. Prints JSON array.
    Tasks,
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct RegisterCrabBody {
    crab_id: String,
    repo_id: String,
    name: String,
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
    token_usage: Option<TokenUsageBody>,
}

#[derive(Debug, Serialize)]
struct TimingBody {
    end_to_end_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TokenUsageBody {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TaskRecord {
    task_id: String,
    mission_id: String,
    title: String,
    assigned_crab_id: Option<String>,
    status: String,
    step_id: Option<String>,
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
        Cmd::Register { repo_id, name, crab_id } => {
            cmd_register(cp, &repo_id, &name, crab_id).await?;
        }
        Cmd::Poll { crab_id } => {
            cmd_poll(cp, &crab_id).await?;
        }
        Cmd::StartRun { mission_id, task_id, crab_id, burrow_path } => {
            cmd_start_run(cp, &mission_id, &task_id, &crab_id, &burrow_path).await?;
        }
        Cmd::CompleteRun {
            run_id,
            status,
            summary,
            result,
            duration_ms,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        } => {
            cmd_complete_run(
                cp,
                &run_id,
                &status,
                summary,
                result,
                duration_ms,
                prompt_tokens,
                completion_tokens,
                total_tokens,
            )
            .await?;
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
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

async fn cmd_register(
    cp: &str,
    repo_id: &str,
    name: &str,
    crab_id: Option<String>,
) -> Result<()> {
    let http = Client::new();
    let crab_id = crab_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let resp = http
        .post(format!("{cp}/v1/crabs/register"))
        .json(&RegisterCrabBody {
            crab_id,
            repo_id: repo_id.to_string(),
            name: name.to_string(),
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
        r#"You are a crab agent in Crabitat. You interact with the control-plane using the `crabitat-crab` CLI and execute tasks yourself using your own tools (Read, Write, Edit, Bash, Glob, Grep).

## Step 1: Register

Pick a name for yourself, then register:

```
crabitat-crab register --control-plane {cp} --repo-id <REPO_ID> --name <NAME>
```

This prints JSON with your `crab_id`. Save it — you need it for all subsequent commands.

To find available repos, run: `crabitat-crab status --control-plane {cp}`

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

Optional but recommended: report token usage with `--prompt-tokens <N> --completion-tokens <N>` (or `--total-tokens <N>`).

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

#[allow(clippy::too_many_arguments)]
async fn cmd_complete_run(
    cp: &str,
    run_id: &str,
    status: &str,
    summary: Option<String>,
    result: Option<String>,
    duration_ms: Option<u64>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
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

    let token_usage =
        if prompt_tokens.is_some() || completion_tokens.is_some() || total_tokens.is_some() {
            Some(TokenUsageBody { prompt_tokens, completion_tokens, total_tokens })
        } else {
            None
        };

    let resp = http
        .post(format!("{cp}/v1/runs/complete"))
        .json(&CompleteRunBody {
            run_id: run_id.to_string(),
            status: status.to_string(),
            summary: final_summary,
            timing: duration_ms.map(|ms| TimingBody { end_to_end_ms: Some(ms) }),
            token_usage,
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
