mod db;
mod error;
mod github;
mod handlers;
mod poller;
mod scheduler;
mod types;
mod workflows;

use db::*;
use github::*;
use handlers::*;
use poller::*;
use types::*;
use workflows::*;

use anyhow::Result;
use axum::{
    Json, Router,
    routing::{delete, get, post},
};
use clap::{Parser, Subcommand};
use std::{
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "crabitat-control-plane", about = "Crabitat control-plane service")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value_t = 8800)]
        port: u16,
        #[arg(long, default_value = "./var/crabitat-control-plane.db")]
        db_path: PathBuf,
        #[arg(long, default_value = "./agent-prompts")]
        prompts_path: PathBuf,
    },
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
        Command::Serve { port, db_path, prompts_path } => {
            serve(port, &db_path, &prompts_path).await?;
        }
    }

    Ok(())
}

async fn serve(port: u16, db_path: &StdPath, prompts_path: &StdPath) -> Result<()> {
    info!("crabitat control-plane v{}", env!("CARGO_PKG_VERSION"));

    let connection = init_db(db_path)?;
    seed_default_workflows(&connection)?;
    seed_settings(&connection, prompts_path)?;
    let workflows = WorkflowRegistry::load(prompts_path);
    info!(count = workflows.manifests.len(), "workflow registry loaded");

    // Assemble workflows based on repo stacks
    let mut workflows = workflows;
    let all_stacks = query_all_repo_stacks(&connection).unwrap_or_default();
    let combo_count = all_stacks.iter().filter(|s| !s.is_empty()).count();
    assemble_workflows(&mut workflows, all_stacks);
    info!(combo_count, total = workflows.manifests.len(), "assembled workflows");

    // Auto-sync TOML + assembled workflows to DB on startup
    let sync_result = sync_toml_workflows_to_db(&connection, &workflows);
    info!(
        synced = sync_result.synced,
        removed = sync_result.removed,
        commit = ?sync_result.commit_hash,
        errors = sync_result.errors.len(),
        "startup workflow sync"
    );

    let github = GitHubClient::new();
    if github.has_token() {
        info!("GitHub: using GraphQL API (GITHUB_TOKEN set)");
    } else {
        info!("GitHub: using gh CLI fallback (set GITHUB_TOKEN for API mode)");
    }
    let state = AppState {
        db: Arc::new(Mutex::new(connection)),
        workflows: Arc::new(RwLock::new(workflows)),
        github,
    };

    let app = build_router(state.clone());

    // Spawn background merge-wait poller
    tokio::spawn(spawn_merge_wait_poller(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on http://{}", addr);
    info!("database: {}", db_path.display());
    info!("prompts:  {}", prompts_path.display());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/repos", post(create_repo).get(list_repos))
        .route("/v1/repos/{repo_id}", get(get_repo).delete(delete_repo))
        .route("/v1/repos/{repo_id}/update", post(update_repo))
        .route("/v1/repos/{repo_id}/issues", get(list_repo_issues))
        .route("/v1/repos/{repo_id}/queue", get(list_queue).post(queue_issue))
        .route("/v1/repos/{repo_id}/queue/{mission_id}", delete(remove_from_queue))
        .route("/v1/crabs", get(list_crabs))
        .route("/v1/crabs/register", post(register_crab))
        .route("/v1/missions", post(create_mission).get(list_missions))
        .route("/v1/missions/{mission_id}", get(get_mission))
        .route("/v1/tasks", post(create_task).get(list_tasks))
        .route("/v1/runs/start", post(start_run))
        .route("/v1/runs/update", post(update_run))
        .route("/v1/runs/complete", post(complete_run))
        .route("/v1/workflows", get(list_db_workflows).post(create_workflow))
        .route("/v1/workflows/sync", post(sync_workflows))
        .route(
            "/v1/workflows/{workflow_id}",
            get(get_workflow).delete(delete_workflow),
        )
        .route("/v1/prompt-files", get(list_prompt_files))
        .route("/v1/prompt-files/preview", get(preview_prompt_file))
        .route("/v1/stacks", get(list_stacks))
        .route("/v1/workflows/{workflow_id}/update", post(update_workflow))
        .route("/v1/settings", get(get_settings).post(patch_settings))
        .route("/v1/repos/{repo_id}/languages", get(get_repo_languages))
        .route("/v1/skills", get(list_skills))
        .route("/v1/status", get(get_status))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use crabitat_core::{BurrowMode, RunStatus};
    use rusqlite::Connection;
    use std::collections::HashMap;

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
        let workflows = WorkflowRegistry {
            manifests: HashMap::new(),
            prompts_path: PathBuf::from("/tmp/test-prompts"),
            stack_map: HashMap::new(),
        };
        AppState {
            db: Arc::new(Mutex::new(conn)),
            workflows: Arc::new(RwLock::new(workflows)),
            github: GitHubClient { http: reqwest::Client::new(), token: None },
        }
    }

    async fn setup_repo(state: &AppState) -> RepoRecord {
        create_repo(
            State(state.clone()),
            Json(CreateRepoRequest {
                owner: "test".into(),
                name: "repo".into(),
                default_branch: Some("main".into()),
                language: None,
                local_path: "/tmp/test".into(),
                stacks: None,
            }),
        )
        .await
        .unwrap()
        .0
    }

    #[tokio::test]
    async fn register_and_list_crabs() {
        let state = test_state();
        let repo = setup_repo(&state).await;

        let crab = register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                repo_id: repo.repo_id.clone(),
                name: "Alice".into(),
                state: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(crab.crab_id, "crab-1");
        assert_eq!(crab.name, "Alice");
        assert_eq!(crab.repo_id, repo.repo_id);
        assert!(matches!(crab.state, CrabState::Idle));

        let crabs = list_crabs(State(state.clone())).await.unwrap().0;
        assert_eq!(crabs.len(), 1);
    }

    #[tokio::test]
    async fn create_mission_and_task() {
        let state = test_state();
        let repo = setup_repo(&state).await;

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                repo_id: repo.repo_id.clone(),
                prompt: "Implement feature X".into(),
                workflow: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(!mission.mission_id.is_empty());
        assert_eq!(mission.repo_id, repo.repo_id);

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                repo_id: repo.repo_id.clone(),
                name: "Alice".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        let task = create_task(
            State(state.clone()),
            Json(CreateTaskRequest {
                mission_id: mission.mission_id.clone(),
                title: "Write tests".into(),
                assigned_crab_id: Some("crab-1".into()),
                status: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(task.title, "Write tests");
        assert_eq!(task.assigned_crab_id.as_deref(), Some("crab-1"));
    }

    #[tokio::test]
    async fn full_run_lifecycle() {
        let state = test_state();
        let repo = setup_repo(&state).await;

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                repo_id: repo.repo_id.clone(),
                name: "Alice".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                repo_id: repo.repo_id.clone(),
                prompt: "Build feature".into(),
                workflow: None,
            }),
        )
        .await
        .unwrap()
        .0;

        let task = create_task(
            State(state.clone()),
            Json(CreateTaskRequest {
                mission_id: mission.mission_id.clone(),
                title: "Implement it".into(),
                assigned_crab_id: None,
                status: None,
            }),
        )
        .await
        .unwrap()
        .0;

        // Start a run
        let run = start_run(
            State(state.clone()),
            Json(StartRunRequest {
                run_id: None,
                mission_id: mission.mission_id.clone(),
                task_id: task.task_id.clone(),
                crab_id: "crab-1".into(),
                burrow_path: "/tmp/burrow-1".into(),
                burrow_mode: BurrowMode::Worktree,
                status: None,
                progress_message: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(matches!(run.status, RunStatus::Running));

        // Update the run
        let updated = update_run(
            State(state.clone()),
            Json(UpdateRunRequest {
                run_id: run.run_id.clone(),
                status: None,
                progress_message: Some("halfway there".into()),
                token_usage: Some(TokenUsagePatch {
                    prompt_tokens: Some(100),
                    completion_tokens: Some(50),
                    total_tokens: None,
                }),
                timing: Some(TimingPatch {
                    first_token_ms: Some(200),
                    llm_duration_ms: None,
                    execution_duration_ms: None,
                    end_to_end_ms: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(updated.progress_message, "halfway there");
        assert_eq!(updated.metrics.prompt_tokens, 100);
        assert_eq!(updated.metrics.completion_tokens, 50);
        assert_eq!(updated.metrics.total_tokens, 150);

        // Complete the run
        let completed = complete_run(
            State(state.clone()),
            Json(CompleteRunRequest {
                run_id: run.run_id.clone(),
                status: RunStatus::Completed,
                summary: Some("All done".into()),
                token_usage: Some(TokenUsagePatch {
                    prompt_tokens: Some(200),
                    completion_tokens: Some(100),
                    total_tokens: None,
                }),
                timing: Some(TimingPatch {
                    first_token_ms: None,
                    llm_duration_ms: Some(1500),
                    execution_duration_ms: Some(3000),
                    end_to_end_ms: Some(5000),
                }),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(matches!(completed.status, RunStatus::Completed));
        assert_eq!(completed.summary.as_deref(), Some("All done"));
        assert!(completed.completed_at_ms.is_some());
    }

    #[tokio::test]
    async fn status_snapshot_totals() {
        let state = test_state();
        let repo = setup_repo(&state).await;

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                repo_id: repo.repo_id.clone(),
                name: "Alice".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-2".into(),
                repo_id: repo.repo_id.clone(),
                name: "Bob".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                repo_id: repo.repo_id.clone(),
                prompt: "Test mission".into(),
                workflow: None,
            }),
        )
        .await
        .unwrap()
        .0;

        let task = create_task(
            State(state.clone()),
            Json(CreateTaskRequest {
                mission_id: mission.mission_id.clone(),
                title: "Test task".into(),
                assigned_crab_id: None,
                status: None,
            }),
        )
        .await
        .unwrap()
        .0;

        let run = start_run(
            State(state.clone()),
            Json(StartRunRequest {
                run_id: None,
                mission_id: mission.mission_id.clone(),
                task_id: task.task_id.clone(),
                crab_id: "crab-1".into(),
                burrow_path: "/tmp/b1".into(),
                burrow_mode: BurrowMode::Worktree,
                status: None,
                progress_message: None,
            }),
        )
        .await
        .unwrap()
        .0;

        complete_run(
            State(state.clone()),
            Json(CompleteRunRequest {
                run_id: run.run_id.clone(),
                status: RunStatus::Completed,
                summary: Some("done".into()),
                token_usage: Some(TokenUsagePatch {
                    prompt_tokens: Some(500),
                    completion_tokens: Some(300),
                    total_tokens: None,
                }),
                timing: Some(TimingPatch {
                    first_token_ms: None,
                    llm_duration_ms: None,
                    execution_duration_ms: None,
                    end_to_end_ms: Some(4000),
                }),
            }),
        )
        .await
        .unwrap();

        let snapshot = get_status(State(state.clone())).await.unwrap().0;

        assert_eq!(snapshot.summary.total_crabs, 2);
        assert_eq!(snapshot.summary.busy_crabs, 0);
        assert_eq!(snapshot.summary.completed_runs, 1);
        assert_eq!(snapshot.summary.failed_runs, 0);
        assert_eq!(snapshot.summary.total_tokens, 800);
        assert_eq!(snapshot.summary.avg_end_to_end_ms, Some(4000));
    }

    #[tokio::test]
    async fn get_mission_by_id() {
        let state = test_state();
        let repo = setup_repo(&state).await;

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                repo_id: repo.repo_id.clone(),
                prompt: "Implement feature Y".into(),
                workflow: None,
            }),
        )
        .await
        .unwrap()
        .0;

        let fetched =
            get_mission(State(state.clone()), Path(mission.mission_id.clone())).await.unwrap().0;

        assert_eq!(fetched.mission_id, mission.mission_id);
        assert_eq!(fetched.repo_id, repo.repo_id);
        assert_eq!(fetched.prompt, "Implement feature Y");
    }

    #[tokio::test]
    async fn get_mission_not_found() {
        let state = test_state();
        let result =
            get_mission(State(state.clone()), Path("00000000-0000-0000-0000-000000000000".into()))
                .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }
}
