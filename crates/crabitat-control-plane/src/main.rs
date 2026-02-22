use anyhow::Result;
use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use clap::{Parser, Subcommand};
use crabitat_core::{
    BurrowMode, Colony, Mission, MissionId, MissionStatus, RunId, RunMetrics, RunStatus, TaskId,
    TaskStatus, WorkflowManifest, evaluate_condition, now_ms,
};
use crabitat_protocol::{Envelope, MessageKind};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, broadcast, mpsc};
use tower_http::cors::CorsLayer;
use tracing::info;
use uuid::Uuid;

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

type CrabChannels = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<String>>>>;

// ---------------------------------------------------------------------------
// Workflow Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct WorkflowRegistry {
    manifests: HashMap<String, WorkflowManifest>,
    prompts_path: PathBuf,
}

impl WorkflowRegistry {
    fn load(prompts_path: &StdPath) -> Self {
        let mut manifests = HashMap::new();
        let workflows_dir = prompts_path.join("workflows");

        if workflows_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&workflows_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    match fs::read_to_string(&path) {
                        Ok(content) => match toml::from_str::<WorkflowManifest>(&content) {
                            Ok(manifest) => {
                                info!(
                                    name = %manifest.workflow.name,
                                    steps = manifest.steps.len(),
                                    "loaded workflow"
                                );
                                manifests.insert(manifest.workflow.name.clone(), manifest);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    err = %e,
                                    "failed to parse workflow TOML"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                err = %e,
                                "failed to read workflow file"
                            );
                        }
                    }
                }
            }
        }

        Self { manifests, prompts_path: prompts_path.to_path_buf() }
    }

    fn get(&self, name: &str) -> Option<&WorkflowManifest> {
        self.manifests.get(name)
    }

    fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.manifests.keys().cloned().collect();
        names.sort();
        names
    }

    fn load_prompt_file(&self, prompt_file: &str) -> Result<String, ApiError> {
        let path = self.prompts_path.join(prompt_file);
        fs::read_to_string(&path).map_err(|e| {
            ApiError::internal(format!("failed to read prompt file {}: {e}", path.display()))
        })
    }
}

// ---------------------------------------------------------------------------
// GitHub Client (GraphQL API with gh CLI fallback)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct GitHubClient {
    http: reqwest::Client,
    token: Option<String>,
}

/// Unified issue type returned by both backends.
#[derive(Debug, Clone)]
struct GhIssue {
    number: i64,
    title: String,
    body: String,
    labels: Vec<String>,
    state: String,
}

/// Unified issue-detail type (title + body only).
#[derive(Debug, Clone)]
struct GhIssueDetail {
    title: String,
    body: String,
}

/// Unified PR-status type.
#[derive(Debug, Clone)]
struct GhPrStatus {
    state: String,
    merged_at: Option<String>,
}

// -- GraphQL response deserialization helpers --------------------------------

#[derive(Debug, Deserialize)]
struct GqlIssue {
    number: i64,
    title: String,
    body: String,
    labels: GqlLabels,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GqlLabels {
    nodes: Vec<GqlLabel>,
}

#[derive(Debug, Deserialize)]
struct GqlLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GqlIssueDetail {
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPrStatus {
    state: String,
    merged_at: Option<String>,
}

// -- gh CLI response deserialization helpers ---------------------------------

#[derive(Debug, Deserialize)]
struct CliLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CliIssue {
    number: i64,
    title: String,
    body: String,
    labels: Vec<CliLabel>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct CliIssueDetail {
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliPrStatus {
    state: String,
    merged_at: Option<String>,
}

impl GitHubClient {
    fn new() -> Self {
        Self { http: reqwest::Client::new(), token: std::env::var("GITHUB_TOKEN").ok() }
    }

    fn has_token(&self) -> bool {
        self.token.is_some()
    }

    // -- Public API (dispatches to GraphQL or gh CLI) -----------------------

    async fn list_issues(&self, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.list_issues_graphql(owner, name).await
        } else {
            self.list_issues_cli(repo).await
        }
    }

    async fn get_issue(&self, repo: &str, number: i64) -> Result<GhIssueDetail, ApiError> {
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.get_issue_graphql(owner, name, number).await
        } else {
            self.get_issue_cli(repo, number).await
        }
    }

    async fn get_pr_status(&self, repo: &str, number: i64) -> Result<GhPrStatus, ApiError> {
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.get_pr_status_graphql(owner, name, number).await
        } else {
            self.get_pr_status_cli(repo, number).await
        }
    }

    // -- GraphQL backend ----------------------------------------------------

    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        let token = self
            .token
            .as_deref()
            .ok_or_else(|| ApiError::internal("graphql called without GITHUB_TOKEN"))?;

        let resp = self
            .http
            .post("https://api.github.com/graphql")
            .bearer_auth(token)
            .header("User-Agent", "crabitat-control-plane")
            .json(&serde_json::json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("GitHub API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!("GitHub API returned {status}: {body}")));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("failed to parse GitHub response: {e}")))?;

        if let Some(errors) = body.get("errors") {
            return Err(ApiError::internal(format!("GitHub GraphQL errors: {errors}")));
        }

        Ok(body)
    }

    async fn list_issues_graphql(&self, owner: &str, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        let query = r#"
            query($owner: String!, $repo: String!) {
                repository(owner: $owner, name: $repo) {
                    issues(first: 50, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) {
                        nodes {
                            number
                            title
                            body
                            labels(first: 10) { nodes { name } }
                            state
                        }
                    }
                }
            }
        "#;

        let body = self.graphql(query, serde_json::json!({ "owner": owner, "repo": repo })).await?;

        let nodes = body
            .pointer("/data/repository/issues/nodes")
            .ok_or_else(|| ApiError::internal("unexpected GitHub response structure"))?;

        let gql_issues: Vec<GqlIssue> = serde_json::from_value(nodes.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse issues: {e}")))?;

        Ok(gql_issues
            .into_iter()
            .map(|i| GhIssue {
                number: i.number,
                title: i.title,
                body: i.body,
                labels: i.labels.nodes.into_iter().map(|l| l.name).collect(),
                state: i.state,
            })
            .collect())
    }

    async fn get_issue_graphql(
        &self,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<GhIssueDetail, ApiError> {
        let query = r#"
            query($owner: String!, $repo: String!, $number: Int!) {
                repository(owner: $owner, name: $repo) {
                    issue(number: $number) {
                        title
                        body
                    }
                }
            }
        "#;

        let body = self
            .graphql(query, serde_json::json!({ "owner": owner, "repo": repo, "number": number }))
            .await?;

        let issue = body
            .pointer("/data/repository/issue")
            .ok_or_else(|| ApiError::internal("issue not found in GitHub response"))?;

        let d: GqlIssueDetail = serde_json::from_value(issue.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse issue: {e}")))?;

        Ok(GhIssueDetail { title: d.title, body: d.body })
    }

    async fn get_pr_status_graphql(
        &self,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<GhPrStatus, ApiError> {
        let query = r#"
            query($owner: String!, $repo: String!, $number: Int!) {
                repository(owner: $owner, name: $repo) {
                    pullRequest(number: $number) {
                        state
                        mergedAt
                    }
                }
            }
        "#;

        let body = self
            .graphql(query, serde_json::json!({ "owner": owner, "repo": repo, "number": number }))
            .await?;

        let pr = body
            .pointer("/data/repository/pullRequest")
            .ok_or_else(|| ApiError::internal("PR not found in GitHub response"))?;

        let s: GqlPrStatus = serde_json::from_value(pr.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse PR status: {e}")))?;

        Ok(GhPrStatus { state: s.state, merged_at: s.merged_at })
    }

    // -- gh CLI backend -----------------------------------------------------

    async fn list_issues_cli(&self, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        let output = tokio::process::Command::new("gh")
            .args([
                "issue",
                "list",
                "--repo",
                repo,
                "--json",
                "number,title,body,labels,state",
                "--state",
                "open",
                "--limit",
                "50",
            ])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh issue list failed: {stderr}")));
        }

        let issues: Vec<CliIssue> = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(issues
            .into_iter()
            .map(|i| GhIssue {
                number: i.number,
                title: i.title,
                body: i.body,
                labels: i.labels.into_iter().map(|l| l.name).collect(),
                state: i.state,
            })
            .collect())
    }

    async fn get_issue_cli(&self, repo: &str, number: i64) -> Result<GhIssueDetail, ApiError> {
        let output = tokio::process::Command::new("gh")
            .args(["issue", "view", &number.to_string(), "--repo", repo, "--json", "title,body"])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh issue view failed: {stderr}")));
        }

        let d: CliIssueDetail = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(GhIssueDetail { title: d.title, body: d.body })
    }

    async fn get_pr_status_cli(&self, repo: &str, number: i64) -> Result<GhPrStatus, ApiError> {
        let output = tokio::process::Command::new("gh")
            .args(["pr", "view", &number.to_string(), "--repo", repo, "--json", "state,mergedAt"])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh pr view failed: {stderr}")));
        }

        let s: CliPrStatus = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(GhPrStatus { state: s.state, merged_at: s.merged_at })
    }
}

fn parse_repo(repo: &str) -> Result<(&str, &str), ApiError> {
    repo.split_once('/').ok_or_else(|| ApiError::bad_request("repo must be in 'owner/repo' format"))
}

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    crab_channels: CrabChannels,
    console_tx: broadcast::Sender<String>,
    workflows: Arc<WorkflowRegistry>,
    github: GitHubClient,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ConsoleEvent {
    Snapshot(StatusSnapshot),
    CrabUpdated { crab: CrabRecord },
    ColonyCreated { colony: ColonyRecord },
    MissionCreated { mission: MissionRecord },
    MissionUpdated { mission: MissionRecord },
    TaskCreated { task: TaskRecord },
    TaskUpdated { task: TaskRecord },
    RunCreated { run: RunRecord },
    RunUpdated { run: RunRecord },
    RunCompleted { run: RunRecord },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CrabState {
    Idle,
    Busy,
    Offline,
}

impl CrabState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Offline => "offline",
        }
    }

    fn from_str(raw: &str) -> Self {
        match raw {
            "busy" => Self::Busy,
            "offline" => Self::Offline,
            _ => Self::Idle,
        }
    }
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    ok: bool,
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.into() }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: message.into() }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() }
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(value: rusqlite::Error) -> Self {
        Self::internal(format!("sqlite error: {value}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ApiErrorBody { ok: false, error: self.message })).into_response()
    }
}

// ---------------------------------------------------------------------------
// Record types (API responses)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct ColonyRecord {
    colony_id: String,
    name: String,
    description: String,
    repo: Option<String>,
    created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CrabRecord {
    crab_id: String,
    colony_id: String,
    name: String,
    role: String,
    state: CrabState,
    current_task_id: Option<String>,
    current_run_id: Option<String>,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MissionRecord {
    mission_id: String,
    colony_id: String,
    prompt: String,
    workflow_name: Option<String>,
    status: MissionStatus,
    worktree_path: Option<String>,
    queue_position: Option<i64>,
    github_issue_number: Option<i64>,
    github_pr_number: Option<i64>,
    created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct TaskRecord {
    task_id: String,
    mission_id: String,
    title: String,
    assigned_crab_id: Option<String>,
    status: TaskStatus,
    step_id: Option<String>,
    role: Option<String>,
    prompt: Option<String>,
    context: Option<String>,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct RunRecord {
    run_id: String,
    mission_id: String,
    task_id: String,
    crab_id: String,
    status: RunStatus,
    burrow_path: String,
    burrow_mode: BurrowMode,
    progress_message: String,
    summary: Option<String>,
    metrics: RunMetrics,
    started_at_ms: u64,
    updated_at_ms: u64,
    completed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct StatusSummary {
    total_crabs: usize,
    busy_crabs: usize,
    running_tasks: usize,
    running_runs: usize,
    completed_runs: usize,
    failed_runs: usize,
    total_tokens: u64,
    avg_end_to_end_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct StatusSnapshot {
    generated_at_ms: u64,
    summary: StatusSummary,
    colonies: Vec<ColonyRecord>,
    crabs: Vec<CrabRecord>,
    missions: Vec<MissionRecord>,
    tasks: Vec<TaskRecord>,
    runs: Vec<RunRecord>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateColonyRequest {
    name: String,
    description: Option<String>,
    repo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegisterCrabRequest {
    crab_id: String,
    colony_id: String,
    name: String,
    role: String,
    state: Option<CrabState>,
}

#[derive(Debug, Deserialize)]
struct CreateMissionRequest {
    colony_id: String,
    prompt: String,
    workflow: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    mission_id: String,
    title: String,
    assigned_crab_id: Option<String>,
    status: Option<TaskStatus>,
}

#[derive(Debug, Deserialize)]
struct StartRunRequest {
    run_id: Option<String>,
    mission_id: String,
    task_id: String,
    crab_id: String,
    burrow_path: String,
    burrow_mode: BurrowMode,
    status: Option<RunStatus>,
    progress_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenUsagePatch {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TimingPatch {
    first_token_ms: Option<u64>,
    llm_duration_ms: Option<u64>,
    execution_duration_ms: Option<u64>,
    end_to_end_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct UpdateRunRequest {
    run_id: String,
    status: Option<RunStatus>,
    progress_message: Option<String>,
    token_usage: Option<TokenUsagePatch>,
    timing: Option<TimingPatch>,
}

#[derive(Debug, Deserialize)]
struct CompleteRunRequest {
    run_id: String,
    status: RunStatus,
    summary: Option<String>,
    token_usage: Option<TokenUsagePatch>,
    timing: Option<TimingPatch>,
}

#[derive(Debug, Deserialize)]
struct UpdateColonyRequest {
    repo: Option<String>,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GitHubIssueRecord {
    number: i64,
    title: String,
    body: String,
    labels: Vec<String>,
    state: String,
    already_queued: bool,
}

#[derive(Debug, Deserialize)]
struct QueueIssueRequest {
    issue_number: i64,
    workflow: Option<String>,
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
    let (console_tx, _) = broadcast::channel::<String>(256);
    let workflows = WorkflowRegistry::load(prompts_path);
    info!(count = workflows.manifests.len(), "workflow registry loaded");
    let github = GitHubClient::new();
    if github.has_token() {
        info!("GitHub: using GraphQL API (GITHUB_TOKEN set)");
    } else {
        info!("GitHub: using gh CLI fallback (set GITHUB_TOKEN for API mode)");
    }
    let state = AppState {
        db: Arc::new(Mutex::new(connection)),
        crab_channels: Arc::new(Mutex::new(HashMap::new())),
        console_tx,
        workflows: Arc::new(workflows),
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
        .route("/v1/colonies", post(create_colony).get(list_colonies))
        .route("/v1/colonies/{colony_id}", patch(update_colony))
        .route("/v1/colonies/{colony_id}/issues", get(list_colony_issues))
        .route("/v1/colonies/{colony_id}/queue", get(list_queue).post(queue_issue))
        .route("/v1/colonies/{colony_id}/queue/{mission_id}", delete(remove_from_queue))
        .route("/v1/crabs", get(list_crabs))
        .route("/v1/crabs/register", post(register_crab))
        .route("/v1/missions", post(create_mission).get(list_missions))
        .route("/v1/missions/{mission_id}", get(get_mission))
        .route("/v1/tasks", post(create_task).get(list_tasks))
        .route("/v1/runs/start", post(start_run))
        .route("/v1/runs/update", post(update_run))
        .route("/v1/runs/complete", post(complete_run))
        .route("/v1/workflows", get(list_workflows))
        .route("/v1/status", get(get_status))
        .route("/v1/ws/crab/{crab_id}", get(ws_crab_handler))
        .route("/v1/ws/console", get(ws_console_handler))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

fn apply_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS colonies (
          colony_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          repo TEXT,
          created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS crabs (
          crab_id TEXT PRIMARY KEY,
          colony_id TEXT NOT NULL,
          name TEXT NOT NULL,
          role TEXT NOT NULL,
          state TEXT NOT NULL,
          current_task_id TEXT,
          current_run_id TEXT,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(colony_id) REFERENCES colonies(colony_id)
        );

        CREATE TABLE IF NOT EXISTS missions (
          mission_id TEXT PRIMARY KEY,
          colony_id TEXT NOT NULL,
          prompt TEXT NOT NULL,
          workflow_name TEXT,
          status TEXT NOT NULL DEFAULT 'pending',
          worktree_path TEXT,
          queue_position INTEGER,
          github_issue_number INTEGER,
          github_pr_number INTEGER,
          created_at_ms INTEGER NOT NULL,
          FOREIGN KEY(colony_id) REFERENCES colonies(colony_id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
          task_id TEXT PRIMARY KEY,
          mission_id TEXT NOT NULL,
          title TEXT NOT NULL,
          assigned_crab_id TEXT,
          status TEXT NOT NULL,
          step_id TEXT,
          role TEXT,
          prompt TEXT,
          context TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(mission_id) REFERENCES missions(mission_id)
        );

        CREATE TABLE IF NOT EXISTS task_deps (
          task_id TEXT NOT NULL,
          depends_on_task_id TEXT NOT NULL,
          PRIMARY KEY (task_id, depends_on_task_id)
        );

        CREATE TABLE IF NOT EXISTS runs (
          run_id TEXT PRIMARY KEY,
          mission_id TEXT NOT NULL,
          task_id TEXT NOT NULL,
          crab_id TEXT NOT NULL,
          status TEXT NOT NULL,
          burrow_path TEXT NOT NULL,
          burrow_mode TEXT NOT NULL,
          progress_message TEXT NOT NULL,
          summary TEXT,
          prompt_tokens INTEGER NOT NULL DEFAULT 0,
          completion_tokens INTEGER NOT NULL DEFAULT 0,
          total_tokens INTEGER NOT NULL DEFAULT 0,
          first_token_ms INTEGER,
          llm_duration_ms INTEGER,
          execution_duration_ms INTEGER,
          end_to_end_ms INTEGER,
          started_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          completed_at_ms INTEGER,
          FOREIGN KEY(mission_id) REFERENCES missions(mission_id),
          FOREIGN KEY(task_id) REFERENCES tasks(task_id)
        );
        ",
    )?;

    // Migrations: add columns to existing tables (safe to re-run)
    let migrations = [
        "ALTER TABLE colonies ADD COLUMN repo TEXT",
        "ALTER TABLE missions ADD COLUMN workflow_name TEXT",
        "ALTER TABLE missions ADD COLUMN queue_position INTEGER",
        "ALTER TABLE missions ADD COLUMN github_issue_number INTEGER",
        "ALTER TABLE missions ADD COLUMN github_pr_number INTEGER",
    ];
    for sql in migrations {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn init_db(db_path: &StdPath) -> Result<Connection> {
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    apply_schema(&conn)?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

async fn ws_crab_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(crab_id): Path<String>,
) -> Response {
    info!(crab_id = %crab_id, "WebSocket upgrade requested");
    ws.on_upgrade(move |socket| handle_ws_crab(socket, state, crab_id))
}

async fn handle_ws_crab(mut socket: WebSocket, state: AppState, crab_id: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register the channel for this crab
    {
        let mut channels = state.crab_channels.lock().await;
        channels.insert(crab_id.clone(), tx);
    }
    info!(crab_id = %crab_id, "WebSocket connected");

    loop {
        tokio::select! {
            // Messages from the crab (heartbeats)
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(envelope) = serde_json::from_str::<Envelope>(&text)
                            && let MessageKind::Heartbeat(_) = &envelope.kind
                        {
                            let db = state.db.lock().await;
                            let _ = db.execute(
                                "UPDATE crabs SET updated_at_ms = ?2 WHERE crab_id = ?1",
                                params![crab_id, now_ms() as i64],
                            );
                            if let Ok(Some(crab)) = fetch_crab(&db, &crab_id) {
                                emit_console_event(&state.console_tx, ConsoleEvent::CrabUpdated { crab });
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Messages to the crab (task assignments)
            channel_msg = rx.recv() => {
                match channel_msg {
                    Some(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    // Cleanup on disconnect
    {
        let mut channels = state.crab_channels.lock().await;
        channels.remove(&crab_id);
    }
    {
        let db = state.db.lock().await;
        let _ = db.execute(
            "UPDATE crabs SET state = 'offline', updated_at_ms = ?2 WHERE crab_id = ?1",
            params![crab_id, now_ms() as i64],
        );
    }
    info!(crab_id = %crab_id, "WebSocket disconnected");

    // Emit crab offline event to console subscribers
    {
        let db = state.db.lock().await;
        if let Ok(Some(crab)) = fetch_crab(&db, &crab_id) {
            emit_console_event(&state.console_tx, ConsoleEvent::CrabUpdated { crab });
        }
    }
}

async fn ws_console_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    info!("Console WebSocket upgrade requested");
    ws.on_upgrade(move |socket| handle_ws_console(socket, state))
}

async fn handle_ws_console(mut socket: WebSocket, state: AppState) {
    // Send initial snapshot
    {
        let db = state.db.lock().await;
        if let Ok(snapshot) = build_status_snapshot(&db) {
            let event = ConsoleEvent::Snapshot(snapshot);
            if let Ok(json) = serde_json::to_string(&event)
                && socket.send(Message::Text(json.into())).await.is_err()
            {
                return;
            }
        }
    }

    let mut rx = state.console_tx.subscribe();

    loop {
        tokio::select! {
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            broadcast_msg = rx.recv() => {
                match broadcast_msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Re-send full snapshot on lag
                        let db = state.db.lock().await;
                        if let Ok(snapshot) = build_status_snapshot(&db) {
                            let event = ConsoleEvent::Snapshot(snapshot);
                            if let Ok(json) = serde_json::to_string(&event)
                                && socket.send(Message::Text(json.into())).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    info!("Console WebSocket disconnected");
}

fn emit_console_event(tx: &broadcast::Sender<String>, event: ConsoleEvent) {
    if let Ok(json) = serde_json::to_string(&event) {
        let _ = tx.send(json);
    }
}

async fn dispatch_assignments(state: &AppState, assignments: Vec<SchedulerAssignment>) {
    if assignments.is_empty() {
        return;
    }
    let channels = state.crab_channels.lock().await;
    for assignment in assignments {
        if let Some(tx) = channels.get(&assignment.crab_id)
            && let Ok(json) = serde_json::to_string(&assignment.envelope)
        {
            let _ = tx.send(json);
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn list_workflows(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.workflows.list_names())
}

async fn create_colony(
    State(state): State<AppState>,
    Json(request): Json<CreateColonyRequest>,
) -> Result<Json<ColonyRecord>, ApiError> {
    if request.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    let mut colony = Colony::new(request.name, request.description.unwrap_or_default());
    colony.repo = request.repo.clone();
    let row = ColonyRecord {
        colony_id: colony.id.to_string(),
        name: colony.name,
        description: colony.description,
        repo: colony.repo,
        created_at_ms: colony.created_at_ms,
    };

    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO colonies (colony_id, name, description, repo, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![row.colony_id, row.name, row.description, row.repo, row.created_at_ms],
    )?;

    emit_console_event(&state.console_tx, ConsoleEvent::ColonyCreated { colony: row.clone() });
    Ok(Json(row))
}

async fn list_colonies(State(state): State<AppState>) -> Result<Json<Vec<ColonyRecord>>, ApiError> {
    let db = state.db.lock().await;
    Ok(Json(query_colonies(&db)?))
}

async fn register_crab(
    State(state): State<AppState>,
    Json(request): Json<RegisterCrabRequest>,
) -> Result<Json<CrabRecord>, ApiError> {
    if request.crab_id.trim().is_empty()
        || request.colony_id.trim().is_empty()
        || request.name.trim().is_empty()
        || request.role.trim().is_empty()
    {
        return Err(ApiError::bad_request("crab_id, colony_id, name, and role are required"));
    }

    let (crab, assignments) = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        let colony_exists: i64 = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM colonies WHERE colony_id = ?1)",
            params![request.colony_id],
            |row| row.get(0),
        )?;
        if colony_exists == 0 {
            return Err(ApiError::not_found("colony_id not found"));
        }

        let updated_at_ms = now_ms();
        let crab_state = request.state.unwrap_or(CrabState::Idle);

        // Enforce one crab per role per colony (except "any" which allows multiple)
        if request.role != "any" {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT crab_id FROM crabs WHERE colony_id = ?1 AND role = ?2 AND crab_id != ?3",
                    params![request.colony_id, request.role, request.crab_id],
                    |row| row.get(0),
                )
                .ok();

            if let Some(existing_crab_id) = existing {
                return Err(ApiError::bad_request(format!(
                    "role '{}' is already taken in this colony by crab '{}'",
                    request.role, existing_crab_id
                )));
            }
        }

        tx.execute(
            "
            INSERT INTO crabs (crab_id, colony_id, name, role, state, current_task_id, current_run_id, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6)
            ON CONFLICT(crab_id) DO UPDATE SET
              colony_id=excluded.colony_id,
              name=excluded.name,
              role=excluded.role,
              state=excluded.state,
              updated_at_ms=excluded.updated_at_ms
            ",
            params![
                request.crab_id,
                request.colony_id,
                request.name,
                request.role,
                crab_state.as_str(),
                updated_at_ms
            ],
        )?;

        let crab = fetch_crab(&tx, &request.crab_id)?
            .ok_or_else(|| ApiError::internal("failed to reload crab after registration"))?;
        emit_console_event(&state.console_tx, ConsoleEvent::CrabUpdated { crab: crab.clone() });

        // New idle crab available — run scheduler to assign queued tasks
        let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
        tx.commit().map_err(ApiError::from)?;
        (crab, assignments)
    };

    dispatch_assignments(&state, assignments).await;
    Ok(Json(crab))
}

async fn list_crabs(State(state): State<AppState>) -> Result<Json<Vec<CrabRecord>>, ApiError> {
    let db = state.db.lock().await;
    Ok(Json(query_crabs(&db)?))
}

async fn create_mission(
    State(state): State<AppState>,
    Json(request): Json<CreateMissionRequest>,
) -> Result<Json<MissionRecord>, ApiError> {
    if request.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    if request.colony_id.trim().is_empty() {
        return Err(ApiError::bad_request("colony_id is required"));
    }

    let (row, assignments) = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        let colony_exists: i64 = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM colonies WHERE colony_id = ?1)",
            params![request.colony_id],
            |row| row.get(0),
        )?;
        if colony_exists == 0 {
            return Err(ApiError::not_found("colony_id not found"));
        }

        let mission = Mission::new(&request.prompt);
        let row = MissionRecord {
            mission_id: mission.id.to_string(),
            colony_id: request.colony_id,
            prompt: mission.prompt,
            workflow_name: request.workflow.clone(),
            status: MissionStatus::Pending,
            worktree_path: None,
            queue_position: None,
            github_issue_number: None,
            github_pr_number: None,
            created_at_ms: mission.created_at_ms,
        };

        tx.execute(
            "INSERT INTO missions (mission_id, colony_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.mission_id,
                row.colony_id,
                row.prompt,
                row.workflow_name,
                mission_status_to_db(row.status),
                row.worktree_path,
                row.queue_position,
                row.github_issue_number,
                row.github_pr_number,
                row.created_at_ms
            ],
        )?;

        emit_console_event(
            &state.console_tx,
            ConsoleEvent::MissionCreated { mission: row.clone() },
        );

        // If a workflow is specified, expand it into tasks
        if let Some(ref workflow_name) = request.workflow {
            let manifest = state
                .workflows
                .get(workflow_name)
                .ok_or_else(|| {
                    ApiError::not_found(format!("workflow '{workflow_name}' not found"))
                })?
                .clone();

            let worktree_path = format!("burrows/mission-{}", row.mission_id);
            tx.execute(
                "UPDATE missions SET status = ?2, worktree_path = ?3 WHERE mission_id = ?1",
                params![
                    row.mission_id,
                    mission_status_to_db(MissionStatus::Running),
                    worktree_path
                ],
            )?;

            expand_workflow_into_tasks(
                &tx,
                &state.workflows,
                &manifest,
                &row.mission_id,
                &request.prompt,
                &state.console_tx,
            )?;
        }

        let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
        tx.commit().map_err(ApiError::from)?;
        (row, assignments)
    };

    dispatch_assignments(&state, assignments).await;
    Ok(Json(row))
}

fn expand_workflow_into_tasks(
    conn: &Connection,
    registry: &WorkflowRegistry,
    manifest: &WorkflowManifest,
    mission_id: &str,
    mission_prompt: &str,
    console_tx: &broadcast::Sender<String>,
) -> Result<(), ApiError> {
    let now = now_ms();

    // Map step_id -> task_id for dependency linking
    let mut step_to_task: HashMap<String, String> = HashMap::new();

    for step in &manifest.steps {
        let task_id = TaskId::new().to_string();
        let has_deps = !step.depends_on.is_empty();
        let status = if has_deps { TaskStatus::Blocked } else { TaskStatus::Queued };

        // Load and render the prompt template
        let prompt_template = registry.load_prompt_file(&step.prompt_file).unwrap_or_default();
        let rendered_prompt = prompt_template
            .replace("{{mission_prompt}}", mission_prompt)
            .replace("{{context}}", "")
            .replace("{{worktree_path}}", &format!("burrows/mission-{mission_id}"));

        // Store condition and max_retries in context JSON if present
        let context_json = if step.condition.is_some() || step.max_retries > 0 {
            let mut ctx = serde_json::Map::new();
            if let Some(ref cond) = step.condition {
                ctx.insert("_condition".to_string(), serde_json::Value::String(cond.clone()));
            }
            if step.max_retries > 0 {
                ctx.insert(
                    "_max_retries".to_string(),
                    serde_json::Value::Number(step.max_retries.into()),
                );
            }
            Some(serde_json::to_string(&ctx).unwrap_or_default())
        } else {
            None
        };

        conn.execute(
            "
            INSERT INTO tasks (task_id, mission_id, title, assigned_crab_id, status,
                               step_id, role, prompt, context,
                               created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                task_id,
                mission_id,
                format!("[{}] {}", step.id, step.role),
                task_status_to_db(status),
                step.id,
                step.role,
                rendered_prompt,
                context_json,
                now,
                now
            ],
        )?;

        step_to_task.insert(step.id.clone(), task_id.clone());

        if let Ok(Some(task)) = fetch_task(conn, &task_id) {
            emit_console_event(console_tx, ConsoleEvent::TaskCreated { task });
        }
    }

    // Insert dependency edges
    for step in &manifest.steps {
        if let Some(task_id) = step_to_task.get(&step.id) {
            for dep_step_id in &step.depends_on {
                if let Some(dep_task_id) = step_to_task.get(dep_step_id) {
                    conn.execute(
                        "INSERT INTO task_deps (task_id, depends_on_task_id) VALUES (?1, ?2)",
                        params![task_id, dep_task_id],
                    )?;
                }
            }
        }
    }

    Ok(())
}

async fn list_missions(
    State(state): State<AppState>,
) -> Result<Json<Vec<MissionRecord>>, ApiError> {
    let db = state.db.lock().await;
    let missions = query_missions(&db)?;
    Ok(Json(missions))
}

async fn get_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
) -> Result<Json<MissionRecord>, ApiError> {
    let db = state.db.lock().await;
    let mission =
        fetch_mission(&db, &mission_id)?.ok_or_else(|| ApiError::not_found("mission not found"))?;
    Ok(Json(mission))
}

async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    if request.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    let notify_crab_id = request.assigned_crab_id.clone();

    let (task, mission_prompt) = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        let mission_exists: i64 = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM missions WHERE mission_id = ?1)",
            params![request.mission_id],
            |row| row.get(0),
        )?;
        if mission_exists == 0 {
            return Err(ApiError::not_found("mission_id not found"));
        }

        let created_at_ms = now_ms();
        let status = request.status.unwrap_or(TaskStatus::Queued);
        let task_id = TaskId::new().to_string();

        tx.execute(
            "
            INSERT INTO tasks (task_id, mission_id, title, assigned_crab_id, status,
                               step_id, role, prompt, context,
                               created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            params![
                task_id,
                request.mission_id,
                request.title,
                request.assigned_crab_id,
                task_status_to_db(status),
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                created_at_ms,
                created_at_ms
            ],
        )?;

        if let Some(ref crab_id) = request.assigned_crab_id {
            tx.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?1",
                params![crab_id, task_id, created_at_ms],
            )?;
        }

        let task = fetch_task(&tx, &task_id)?
            .ok_or_else(|| ApiError::internal("failed to reload task after creation"))?;

        // Fetch mission prompt for WebSocket notification
        let mission_prompt = if notify_crab_id.is_some() {
            tx.query_row(
                "SELECT prompt FROM missions WHERE mission_id = ?1",
                params![request.mission_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default()
        } else {
            String::new()
        };

        tx.commit().map_err(ApiError::from)?;
        (task, mission_prompt)
    };

    emit_console_event(&state.console_tx, ConsoleEvent::TaskCreated { task: task.clone() });

    // Push TaskAssigned via WebSocket if the crab is connected
    if let Some(ref crab_id) = notify_crab_id {
        let channels = state.crab_channels.lock().await;
        if let Some(tx) = channels.get(crab_id.as_str()) {
            let task_uuid: Uuid = task.task_id.parse().expect("task_id is a valid uuid");
            let mission_uuid: Uuid = task.mission_id.parse().expect("mission_id is a valid uuid");

            let mut envelope = Envelope::new(
                "control-plane",
                crab_id.as_str(),
                MessageKind::TaskAssigned(crabitat_protocol::TaskAssigned {
                    task_id: TaskId(task_uuid),
                    mission_id: MissionId(mission_uuid),
                    title: task.title.clone(),
                    mission_prompt,
                    desired_status: TaskStatus::Running,
                    step_id: task.step_id.clone(),
                    role: task.role.clone(),
                    prompt: task.prompt.clone(),
                    context: task.context.clone(),
                    worktree_path: None,
                }),
                now_ms(),
            );
            envelope.task_id = Some(TaskId(task_uuid));
            envelope.mission_id = Some(MissionId(mission_uuid));

            if let Ok(json) = serde_json::to_string(&envelope) {
                let _ = tx.send(json);
            }
        }
    }

    Ok(Json(task))
}

async fn list_tasks(State(state): State<AppState>) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    let db = state.db.lock().await;
    Ok(Json(query_tasks(&db)?))
}

async fn start_run(
    State(state): State<AppState>,
    Json(request): Json<StartRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    if request.burrow_path.trim().is_empty() {
        return Err(ApiError::bad_request("burrow_path is required"));
    }

    let run_id = request.run_id.unwrap_or_else(|| RunId::new().to_string());
    let status = request.status.unwrap_or(RunStatus::Running);
    let now = now_ms();
    let progress = request.progress_message.unwrap_or_else(|| "run started".to_string());

    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    let mission_exists: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM missions WHERE mission_id = ?1)",
        params![request.mission_id],
        |row| row.get(0),
    )?;
    if mission_exists == 0 {
        return Err(ApiError::not_found("mission_id not found"));
    }

    let task_exists: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE task_id = ?1)",
        params![request.task_id],
        |row| row.get(0),
    )?;
    if task_exists == 0 {
        return Err(ApiError::not_found("task_id not found"));
    }

    tx.execute(
        "
        INSERT INTO runs (
          run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
          progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
          first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
          started_at_ms, updated_at_ms, completed_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 0, 0, 0, NULL, NULL, NULL, NULL, ?9, ?10, NULL)
        ",
        params![
            run_id,
            request.mission_id,
            request.task_id,
            request.crab_id,
            run_status_to_db(status),
            request.burrow_path,
            burrow_mode_to_db(request.burrow_mode),
            progress,
            now,
            now
        ],
    )
    .map_err(|err| ApiError::bad_request(format!("failed to start run: {err}")))?;

    tx.execute(
        "UPDATE tasks SET assigned_crab_id = ?1, status = ?2, updated_at_ms = ?3 WHERE task_id = ?4",
        params![request.crab_id, task_status_to_db(TaskStatus::Running), now, request.task_id],
    )?;

    tx.execute(
        "UPDATE crabs SET state = 'busy', current_task_id = ?1, current_run_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?4",
        params![request.task_id, run_id, now, request.crab_id],
    )?;

    let run = fetch_run(&tx, &run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after start"))?;
    emit_console_event(&state.console_tx, ConsoleEvent::RunCreated { run: run.clone() });
    tx.commit().map_err(ApiError::from)?;
    Ok(Json(run))
}

async fn update_run(
    State(state): State<AppState>,
    Json(request): Json<UpdateRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    let existing =
        fetch_run(&tx, &request.run_id)?.ok_or_else(|| ApiError::not_found("run_id not found"))?;

    let now = now_ms();
    let status = request.status.unwrap_or(existing.status);
    let progress_message = request.progress_message.unwrap_or(existing.progress_message.clone());
    let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

    tx.execute(
        "
        UPDATE runs
        SET status = ?2,
            progress_message = ?3,
            prompt_tokens = ?4,
            completion_tokens = ?5,
            total_tokens = ?6,
            first_token_ms = ?7,
            llm_duration_ms = ?8,
            execution_duration_ms = ?9,
            end_to_end_ms = ?10,
            updated_at_ms = ?11
        WHERE run_id = ?1
        ",
        params![
            request.run_id,
            run_status_to_db(status),
            progress_message,
            metrics.prompt_tokens,
            metrics.completion_tokens,
            metrics.total_tokens,
            metrics.first_token_ms.map(|v| v as i64),
            metrics.llm_duration_ms.map(|v| v as i64),
            metrics.execution_duration_ms.map(|v| v as i64),
            metrics.end_to_end_ms.map(|v| v as i64),
            now
        ],
    )?;

    match status {
        RunStatus::Running => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Running), now],
            )?;
            tx.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, current_run_id = ?3, updated_at_ms = ?4 WHERE crab_id = ?1",
                params![existing.crab_id, existing.task_id, existing.run_id, now],
            )?;
        }
        RunStatus::Blocked => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Blocked), now],
            )?;
        }
        RunStatus::Completed => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Completed), now],
            )?;
        }
        RunStatus::Failed => {
            tx.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Failed), now],
            )?;
        }
        RunStatus::Queued => {}
    }

    if let Ok(Some(task)) = fetch_task(&tx, &existing.task_id) {
        emit_console_event(&state.console_tx, ConsoleEvent::TaskUpdated { task });
    }

    let updated = fetch_run(&tx, &request.run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after update"))?;
    emit_console_event(&state.console_tx, ConsoleEvent::RunUpdated { run: updated.clone() });
    tx.commit().map_err(ApiError::from)?;
    Ok(Json(updated))
}

async fn complete_run(
    State(state): State<AppState>,
    Json(request): Json<CompleteRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    if !matches!(request.status, RunStatus::Completed | RunStatus::Failed) {
        return Err(ApiError::bad_request(
            "status must be completed or failed for /v1/runs/complete",
        ));
    }

    let (run, assignments) = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        let existing = fetch_run(&tx, &request.run_id)?
            .ok_or_else(|| ApiError::not_found("run_id not found"))?;

        let completed_at = now_ms();
        let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

        tx.execute(
            "
            UPDATE runs
            SET status = ?2,
                summary = ?3,
                prompt_tokens = ?4,
                completion_tokens = ?5,
                total_tokens = ?6,
                first_token_ms = ?7,
                llm_duration_ms = ?8,
                execution_duration_ms = ?9,
                end_to_end_ms = ?10,
                completed_at_ms = ?11,
                updated_at_ms = ?11
            WHERE run_id = ?1
            ",
            params![
                request.run_id,
                run_status_to_db(request.status),
                request.summary,
                metrics.prompt_tokens,
                metrics.completion_tokens,
                metrics.total_tokens,
                metrics.first_token_ms.map(|v| v as i64),
                metrics.llm_duration_ms.map(|v| v as i64),
                metrics.execution_duration_ms.map(|v| v as i64),
                metrics.end_to_end_ms.map(|v| v as i64),
                completed_at
            ],
        )?;

        let task_status = match request.status {
            RunStatus::Completed => TaskStatus::Completed,
            RunStatus::Failed => TaskStatus::Failed,
            _ => TaskStatus::Running,
        };
        tx.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![existing.task_id, task_status_to_db(task_status), completed_at],
        )?;

        tx.execute(
            "UPDATE crabs SET state = 'idle', current_task_id = NULL, current_run_id = NULL, updated_at_ms = ?2 WHERE crab_id = ?1",
            params![existing.crab_id, completed_at],
        )?;

        let run = fetch_run(&tx, &request.run_id)?
            .ok_or_else(|| ApiError::internal("failed to reload run after completion"))?;
        emit_console_event(&state.console_tx, ConsoleEvent::RunCompleted { run: run.clone() });

        cascade_workflow(
            &tx,
            &existing.mission_id,
            &existing.task_id,
            &state.console_tx,
            &state.workflows,
        )?;

        let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
        tx.commit().map_err(ApiError::from)?;
        (run, assignments)
    };

    dispatch_assignments(&state, assignments).await;
    Ok(Json(run))
}

/// After a task completes/fails, check dependent tasks and update their status.
fn cascade_workflow(
    conn: &Connection,
    mission_id: &str,
    completed_task_id: &str,
    console_tx: &broadcast::Sender<String>,
    workflows: &WorkflowRegistry,
) -> Result<(), ApiError> {
    let now = now_ms();

    // Get the completed task's info
    let completed_task = match fetch_task(conn, completed_task_id)? {
        Some(t) => t,
        None => return Ok(()),
    };

    // If this task has no step_id, it's not part of a workflow — skip cascade
    if completed_task.step_id.is_none() {
        return Ok(());
    }

    let completed_step_id = completed_task.step_id.as_deref().unwrap_or("");

    // If the task failed, cascade failure to all dependents
    if matches!(completed_task.status, TaskStatus::Failed) {
        cascade_failure(conn, completed_task_id, now, console_tx)?;
        update_mission_status(conn, mission_id, now, workflows, console_tx)?;
        return Ok(());
    }

    // Build context map from completed runs in this mission
    let context_map = build_context_map(conn, mission_id)?;

    // Find tasks that depend on the completed task
    let mut stmt = conn.prepare("SELECT task_id FROM task_deps WHERE depends_on_task_id = ?1")?;
    let dependent_task_ids: Vec<String> = stmt
        .query_map(params![completed_task_id], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    for dep_task_id in &dependent_task_ids {
        let dep_task = match fetch_task(conn, dep_task_id)? {
            Some(t) => t,
            None => continue,
        };

        // Only process blocked tasks
        if !matches!(dep_task.status, TaskStatus::Blocked) {
            continue;
        }

        // Check if ALL dependencies are terminal (Completed or Skipped)
        let blocked_count: i64 = conn.query_row(
            "
            SELECT COUNT(*) FROM task_deps td
            JOIN tasks t ON td.depends_on_task_id = t.task_id
            WHERE td.task_id = ?1 AND t.status NOT IN ('completed', 'skipped')
            ",
            params![dep_task_id],
            |row| row.get(0),
        )?;

        if blocked_count > 0 {
            continue; // Still has unresolved dependencies
        }

        // All deps done — evaluate condition
        let _step_id = dep_task.step_id.as_deref().unwrap_or("");

        // Look up the condition from the step_id / task's workflow context
        // The condition is stored implicitly — we check if this step has a condition
        // by querying the task's prompt metadata. For now, we look at the task_deps
        // to find the original step's condition from the workflow.
        // Since conditions are evaluated at cascade time, we store them in task context.
        let condition = get_task_condition(conn, dep_task_id)?;

        let should_queue =
            if let Some(cond) = condition { evaluate_condition(&cond, &context_map) } else { true };

        if should_queue {
            // Build accumulated context from dependency chain
            let accumulated_context = build_accumulated_context(conn, dep_task_id)?;

            conn.execute(
                "UPDATE tasks SET status = ?2, context = ?3, updated_at_ms = ?4 WHERE task_id = ?1",
                params![
                    dep_task_id,
                    task_status_to_db(TaskStatus::Queued),
                    accumulated_context,
                    now
                ],
            )?;
        } else {
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![dep_task_id, task_status_to_db(TaskStatus::Skipped), now],
            )?;
        }

        if let Ok(Some(updated_task)) = fetch_task(conn, dep_task_id) {
            emit_console_event(console_tx, ConsoleEvent::TaskUpdated { task: updated_task });
        }

        // If we just skipped a task, recurse to cascade further
        if !should_queue {
            cascade_workflow(conn, mission_id, dep_task_id, console_tx, workflows)?;
        }
    }

    // Handle fix→review retry loop: if a "fix" step completed, find the "review"
    // step that depends on "implement" (same mission) and re-queue it
    if completed_step_id == "fix" {
        requeue_review_after_fix(conn, mission_id, now, console_tx)?;
    }

    // Capture PR number from the "pr" step result
    if completed_step_id == "pr" {
        let context_map = build_context_map(conn, mission_id)?;
        if let Some(pr_num_str) = context_map.get("pr.result")
            && let Ok(pr_num) = pr_num_str.parse::<i64>()
        {
            conn.execute(
                "UPDATE missions SET github_pr_number = ?2 WHERE mission_id = ?1",
                params![mission_id, pr_num],
            )?;
        }
    }

    update_mission_status(conn, mission_id, now, workflows, console_tx)?;
    Ok(())
}

fn cascade_failure(
    conn: &Connection,
    failed_task_id: &str,
    now: u64,
    console_tx: &broadcast::Sender<String>,
) -> Result<(), ApiError> {
    let mut stmt = conn.prepare("SELECT task_id FROM task_deps WHERE depends_on_task_id = ?1")?;
    let dependent_task_ids: Vec<String> =
        stmt.query_map(params![failed_task_id], |row| row.get(0))?.filter_map(Result::ok).collect();

    for dep_task_id in &dependent_task_ids {
        conn.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![dep_task_id, task_status_to_db(TaskStatus::Failed), now],
        )?;
        if let Ok(Some(task)) = fetch_task(conn, dep_task_id) {
            emit_console_event(console_tx, ConsoleEvent::TaskUpdated { task });
        }
        cascade_failure(conn, dep_task_id, now, console_tx)?;
    }
    Ok(())
}

fn requeue_review_after_fix(
    conn: &Connection,
    mission_id: &str,
    now: u64,
    console_tx: &broadcast::Sender<String>,
) -> Result<(), ApiError> {
    // Find the "review" task in this mission and check its retry count
    let review_task: Option<(String, i64)> = conn
        .query_row(
            "
            SELECT task_id,
                   (SELECT COUNT(*) FROM runs WHERE task_id = t.task_id AND status = 'completed') as run_count
            FROM tasks t
            WHERE mission_id = ?1 AND step_id = 'review'
            ",
            params![mission_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((review_task_id, _run_count)) = review_task {
        // Reset review to Queued so it re-runs
        conn.execute(
            "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
            params![review_task_id, task_status_to_db(TaskStatus::Queued), now],
        )?;
        if let Ok(Some(task)) = fetch_task(conn, &review_task_id) {
            emit_console_event(console_tx, ConsoleEvent::TaskUpdated { task });
        }
    }
    Ok(())
}

fn build_context_map(
    conn: &Connection,
    mission_id: &str,
) -> Result<HashMap<String, String>, ApiError> {
    let mut context: HashMap<String, String> = HashMap::new();

    let mut stmt = conn.prepare(
        "
        SELECT t.step_id, r.summary
        FROM tasks t
        JOIN runs r ON r.task_id = t.task_id
        WHERE t.mission_id = ?1 AND r.status = 'completed' AND t.step_id IS NOT NULL
        ORDER BY r.completed_at_ms DESC
        ",
    )?;

    let rows: Vec<(String, String)> = stmt
        .query_map(params![mission_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default()))
        })?
        .filter_map(Result::ok)
        .collect();

    for (step_id, summary) in rows {
        context.insert(format!("{step_id}.summary"), summary.clone());
        // Try to extract a "result" field from the summary (JSON)
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&summary)
            && let Some(result) = val.get("result").and_then(|v| v.as_str())
        {
            context.insert(format!("{step_id}.result"), result.to_string());
        }
    }

    Ok(context)
}

fn build_accumulated_context(conn: &Connection, task_id: &str) -> Result<String, ApiError> {
    // Collect summaries from all transitive dependencies
    let mut summaries = Vec::new();

    let mut stmt = conn.prepare(
        "
        SELECT t.step_id, r.summary
        FROM task_deps td
        JOIN tasks t ON td.depends_on_task_id = t.task_id
        LEFT JOIN runs r ON r.task_id = t.task_id AND r.status = 'completed'
        WHERE td.task_id = ?1
        ORDER BY t.created_at_ms ASC
        ",
    )?;

    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map(params![task_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    for (step_id, summary) in rows {
        let step = step_id.unwrap_or_else(|| "unknown".to_string());
        let sum = summary.unwrap_or_else(|| "(no summary)".to_string());
        summaries.push(format!("## {step}\n{sum}"));
    }

    Ok(summaries.join("\n\n"))
}

fn get_task_condition(conn: &Connection, task_id: &str) -> Result<Option<String>, ApiError> {
    // We store conditions in the workflow manifest. Since we don't persist the condition
    // in the DB, we look at the prompt field which was rendered from the step.
    // A simpler approach: store the condition in an extra column. For now, we check
    // if the task's prompt contains a condition marker.
    // Actually, let's just query by step_id pattern. The condition is evaluated from
    // the workflow manifest at expand time. We'll store it in the task context.
    //
    // For the MVP, we embed the condition in a tasks.context JSON field during expansion.
    // Let's look for it there.
    let context: Option<String> = conn
        .query_row("SELECT context FROM tasks WHERE task_id = ?1", params![task_id], |row| {
            row.get(0)
        })
        .ok();

    if let Some(ctx) = context
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&ctx)
        && let Some(cond) = val.get("_condition").and_then(|v| v.as_str())
    {
        return Ok(Some(cond.to_string()));
    }
    Ok(None)
}

fn update_mission_status(
    conn: &Connection,
    mission_id: &str,
    _now: u64,
    workflows: &WorkflowRegistry,
    console_tx: &broadcast::Sender<String>,
) -> Result<(), ApiError> {
    // Check if all tasks in the mission are terminal
    let non_terminal_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status NOT IN ('completed', 'failed', 'skipped')",
        params![mission_id],
        |row| row.get(0),
    )?;

    if non_terminal_count == 0 {
        // Check if any task failed
        let failed_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status = 'failed'",
            params![mission_id],
            |row| row.get(0),
        )?;

        let new_status =
            if failed_count > 0 { MissionStatus::Failed } else { MissionStatus::Completed };

        conn.execute(
            "UPDATE missions SET status = ?2 WHERE mission_id = ?1",
            params![mission_id, mission_status_to_db(new_status)],
        )?;

        // Emit mission updated
        if let Ok(Some(mission)) = fetch_mission(conn, mission_id) {
            emit_console_event(
                console_tx,
                ConsoleEvent::MissionUpdated { mission: mission.clone() },
            );

            // Try to activate next mission in this colony's queue
            activate_next_mission_in_colony(conn, &mission.colony_id, workflows, console_tx)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Colony-GitHub binding
// ---------------------------------------------------------------------------

async fn update_colony(
    State(state): State<AppState>,
    Path(colony_id): Path<String>,
    Json(request): Json<UpdateColonyRequest>,
) -> Result<Json<ColonyRecord>, ApiError> {
    let db = state.db.lock().await;

    // Verify colony exists
    let existing = query_colonies(&db)?
        .into_iter()
        .find(|c| c.colony_id == colony_id)
        .ok_or_else(|| ApiError::not_found("colony not found"))?;

    // Validate repo format if provided
    if let Some(ref repo) = request.repo
        && !repo.is_empty()
        && repo.matches('/').count() != 1
    {
        return Err(ApiError::bad_request("repo must be in 'owner/repo' format"));
    }

    let name = request.name.unwrap_or(existing.name);
    let description = request.description.unwrap_or(existing.description);
    let repo = if request.repo.is_some() { request.repo } else { existing.repo };

    db.execute(
        "UPDATE colonies SET name = ?2, description = ?3, repo = ?4 WHERE colony_id = ?1",
        params![colony_id, name, description, repo],
    )?;

    let updated =
        ColonyRecord { colony_id, name, description, repo, created_at_ms: existing.created_at_ms };
    Ok(Json(updated))
}

async fn list_colony_issues(
    State(state): State<AppState>,
    Path(colony_id): Path<String>,
) -> Result<Json<Vec<GitHubIssueRecord>>, ApiError> {
    let (repo, queued_issues) = {
        let db = state.db.lock().await;

        let repo: Option<String> = db
            .query_row(
                "SELECT repo FROM colonies WHERE colony_id = ?1",
                params![colony_id],
                |row| row.get(0),
            )
            .map_err(|_| ApiError::not_found("colony not found"))?;

        let repo = repo.ok_or_else(|| ApiError::bad_request("colony has no repo configured"))?;

        let mut stmt = db.prepare(
            "SELECT github_issue_number FROM missions WHERE colony_id = ?1 AND github_issue_number IS NOT NULL",
        )?;
        let queued_issues: Vec<i64> =
            stmt.query_map(params![colony_id], |row| row.get(0))?.filter_map(Result::ok).collect();

        (repo, queued_issues)
    };

    let issues = state.github.list_issues(&repo).await?;

    let records: Vec<GitHubIssueRecord> = issues
        .into_iter()
        .map(|issue| GitHubIssueRecord {
            already_queued: queued_issues.contains(&issue.number),
            number: issue.number,
            title: issue.title,
            body: issue.body,
            labels: issue.labels,
            state: issue.state,
        })
        .collect();

    Ok(Json(records))
}

// ---------------------------------------------------------------------------
// Mission Queue
// ---------------------------------------------------------------------------

async fn queue_issue(
    State(state): State<AppState>,
    Path(colony_id): Path<String>,
    Json(request): Json<QueueIssueRequest>,
) -> Result<Json<MissionRecord>, ApiError> {
    // Phase 1: Validate colony and get repo (brief lock)
    let repo = {
        let db = state.db.lock().await;
        let repo: Option<String> = db
            .query_row(
                "SELECT repo FROM colonies WHERE colony_id = ?1",
                params![colony_id],
                |row| row.get(0),
            )
            .map_err(|_| ApiError::not_found("colony not found"))?;
        repo.ok_or_else(|| ApiError::bad_request("colony has no repo configured"))?
    };

    // Phase 2: Fetch issue details from GitHub (no lock held)
    let detail = state.github.get_issue(&repo, request.issue_number).await?;

    // Phase 3: All DB work in a single transaction
    let (row, assignments) = {
        let mut db = state.db.lock().await;
        let tx = db.transaction().map_err(ApiError::from)?;

        // Check if issue is already queued
        let already_queued: i64 = tx.query_row(
            "SELECT COUNT(*) FROM missions WHERE colony_id = ?1 AND github_issue_number = ?2",
            params![colony_id, request.issue_number],
            |row| row.get(0),
        )?;
        if already_queued > 0 {
            return Err(ApiError::bad_request("issue is already queued in this colony"));
        }

        // Compute queue position
        let max_pos: Option<i64> = tx
            .query_row(
                "SELECT MAX(queue_position) FROM missions WHERE colony_id = ?1",
                params![colony_id],
                |row| row.get(0),
            )
            .unwrap_or(None);
        let queue_position = max_pos.unwrap_or(0) + 1;

        let workflow_name = request.workflow.unwrap_or_else(|| "dev-task".to_string());
        let prompt =
            format!("{}#{}: {}\n\n{}", repo, request.issue_number, detail.title, detail.body);
        let mission = Mission::new(&prompt);
        let row = MissionRecord {
            mission_id: mission.id.to_string(),
            colony_id: colony_id.clone(),
            prompt,
            workflow_name: Some(workflow_name),
            status: MissionStatus::Pending,
            worktree_path: None,
            queue_position: Some(queue_position),
            github_issue_number: Some(request.issue_number),
            github_pr_number: None,
            created_at_ms: mission.created_at_ms,
        };

        tx.execute(
            "INSERT INTO missions (mission_id, colony_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.mission_id,
                row.colony_id,
                row.prompt,
                row.workflow_name,
                mission_status_to_db(row.status),
                row.worktree_path,
                row.queue_position,
                row.github_issue_number,
                row.github_pr_number,
                row.created_at_ms
            ],
        )?;

        emit_console_event(
            &state.console_tx,
            ConsoleEvent::MissionCreated { mission: row.clone() },
        );

        activate_next_mission_in_colony(&tx, &colony_id, &state.workflows, &state.console_tx)?;

        let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
        tx.commit().map_err(ApiError::from)?;
        (row, assignments)
    };

    dispatch_assignments(&state, assignments).await;
    Ok(Json(row))
}

async fn list_queue(
    State(state): State<AppState>,
    Path(colony_id): Path<String>,
) -> Result<Json<Vec<MissionRecord>>, ApiError> {
    let db = state.db.lock().await;

    let mut stmt = db.prepare(
        "SELECT mission_id, colony_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions WHERE colony_id = ?1 AND queue_position IS NOT NULL ORDER BY queue_position ASC",
    )?;
    let rows = stmt.query_map(params![colony_id], |row| {
        Ok(MissionRecord {
            mission_id: row.get(0)?,
            colony_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        })
    })?;

    Ok(Json(rows.filter_map(Result::ok).collect()))
}

async fn remove_from_queue(
    State(state): State<AppState>,
    Path((colony_id, mission_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut db = state.db.lock().await;
    let tx = db.transaction().map_err(ApiError::from)?;

    // Only allow removing pending missions
    let status: String = tx
        .query_row(
            "SELECT status FROM missions WHERE mission_id = ?1 AND colony_id = ?2 AND queue_position IS NOT NULL",
            params![mission_id, colony_id],
            |row| row.get(0),
        )
        .map_err(|_| ApiError::not_found("mission not found in queue"))?;

    if status != "pending" {
        return Err(ApiError::bad_request("can only remove pending missions from queue"));
    }

    tx.execute("DELETE FROM missions WHERE mission_id = ?1", params![mission_id])?;
    tx.commit().map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "ok": true, "deleted": mission_id })))
}

// ---------------------------------------------------------------------------
// Sequential Mission Activation
// ---------------------------------------------------------------------------

fn activate_next_mission_in_colony(
    conn: &Connection,
    colony_id: &str,
    workflows: &WorkflowRegistry,
    console_tx: &broadcast::Sender<String>,
) -> Result<(), ApiError> {
    // Check: any mission in this colony with status = 'running'?
    let running_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM missions WHERE colony_id = ?1 AND status = 'running' AND queue_position IS NOT NULL",
        params![colony_id],
        |row| row.get(0),
    )?;

    if running_count > 0 {
        return Ok(()); // One at a time
    }

    // Find next pending queued mission
    let next: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT mission_id, workflow_name, prompt FROM missions WHERE colony_id = ?1 AND status = 'pending' AND queue_position IS NOT NULL ORDER BY queue_position ASC LIMIT 1",
            params![colony_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    if let Some((mission_id, workflow_name, mission_prompt)) = next {
        let worktree_path = format!("burrows/mission-{mission_id}");
        conn.execute(
            "UPDATE missions SET status = ?2, worktree_path = ?3 WHERE mission_id = ?1",
            params![mission_id, mission_status_to_db(MissionStatus::Running), worktree_path],
        )?;

        // Expand workflow into tasks if workflow is specified
        if let Some(ref wf_name) = workflow_name
            && let Some(manifest) = workflows.get(wf_name)
        {
            expand_workflow_into_tasks(
                conn,
                workflows,
                &manifest.clone(),
                &mission_id,
                &mission_prompt,
                console_tx,
            )?;
        }

        // Emit mission updated
        if let Ok(Some(mission)) = fetch_mission(conn, &mission_id) {
            emit_console_event(console_tx, ConsoleEvent::MissionUpdated { mission });
        }
    }

    Ok(())
}

struct SchedulerAssignment {
    crab_id: String,
    envelope: Envelope,
}

fn run_scheduler_tick_db(
    conn: &Connection,
    console_tx: &broadcast::Sender<String>,
) -> Result<Vec<SchedulerAssignment>, ApiError> {
    let now = now_ms();
    let mut assignments = Vec::new();

    // Get all queued tasks (ordered by created_at_ms)
    let mut task_stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, step_id, role, prompt, context
        FROM tasks
        WHERE status = 'queued'
        ORDER BY created_at_ms ASC
        ",
    )?;

    struct QueuedTask {
        task_id: String,
        mission_id: String,
        title: String,
        step_id: Option<String>,
        role: Option<String>,
        prompt: Option<String>,
        context: Option<String>,
    }

    let queued_tasks: Vec<QueuedTask> = task_stmt
        .query_map([], |row| {
            Ok(QueuedTask {
                task_id: row.get(0)?,
                mission_id: row.get(1)?,
                title: row.get(2)?,
                step_id: row.get(3)?,
                role: row.get(4)?,
                prompt: row.get(5)?,
                context: row.get(6)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    // Get all idle crabs
    let mut crab_stmt = conn.prepare("SELECT crab_id, role FROM crabs WHERE state = 'idle'")?;

    let mut idle_crabs: Vec<(String, String)> = crab_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    for task in &queued_tasks {
        if idle_crabs.is_empty() {
            break;
        }

        // Skip merge-wait tasks — handled by background poller
        if task.step_id.as_deref() == Some("merge-wait") {
            continue;
        }

        // Check that no other task in the same mission is currently Running
        // (worktree conflict prevention for workflow tasks)
        if task.step_id.is_some() {
            let running_in_mission: i64 = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE mission_id = ?1 AND status = 'running'",
                params![task.mission_id],
                |row| row.get(0),
            )?;
            if running_in_mission > 0 {
                continue;
            }
        }

        let task_role = task.role.as_deref().unwrap_or("any");

        // Find a matching crab — prefer exact role match, fall back to "any"
        let crab_idx =
            idle_crabs.iter().position(|(_, crab_role)| crab_role == task_role).or_else(|| {
                idle_crabs
                    .iter()
                    .position(|(_, crab_role)| task_role == "any" || crab_role == "any")
            });

        if let Some(idx) = crab_idx {
            let (crab_id, _) = idle_crabs.remove(idx);

            // Assign the task
            conn.execute(
                "UPDATE tasks SET assigned_crab_id = ?2, status = ?3, updated_at_ms = ?4 WHERE task_id = ?1",
                params![task.task_id, crab_id, task_status_to_db(TaskStatus::Assigned), now],
            )?;

            conn.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?1",
                params![crab_id, task.task_id, now],
            )?;

            // Get worktree_path for this mission
            let worktree_path: Option<String> = conn
                .query_row(
                    "SELECT worktree_path FROM missions WHERE mission_id = ?1",
                    params![task.mission_id],
                    |row| row.get(0),
                )
                .ok();

            // Get mission_prompt
            let mission_prompt: String = conn
                .query_row(
                    "SELECT prompt FROM missions WHERE mission_id = ?1",
                    params![task.mission_id],
                    |row| row.get(0),
                )
                .unwrap_or_default();

            let task_uuid: Uuid = task.task_id.parse().unwrap_or_else(|_| Uuid::new_v4());
            let mission_uuid: Uuid = task.mission_id.parse().unwrap_or_else(|_| Uuid::new_v4());

            let mut envelope = Envelope::new(
                "control-plane",
                &crab_id,
                MessageKind::TaskAssigned(crabitat_protocol::TaskAssigned {
                    task_id: TaskId(task_uuid),
                    mission_id: MissionId(mission_uuid),
                    title: task.title.clone(),
                    mission_prompt,
                    desired_status: TaskStatus::Running,
                    step_id: task.step_id.clone(),
                    role: task.role.clone(),
                    prompt: task.prompt.clone(),
                    context: task.context.clone(),
                    worktree_path,
                }),
                now,
            );
            envelope.task_id = Some(TaskId(task_uuid));
            envelope.mission_id = Some(MissionId(mission_uuid));

            assignments.push(SchedulerAssignment { crab_id: crab_id.clone(), envelope });

            if let Ok(Some(t)) = fetch_task(conn, &task.task_id) {
                emit_console_event(console_tx, ConsoleEvent::TaskUpdated { task: t });
            }
            if let Ok(Some(crab)) = fetch_crab(conn, &crab_id) {
                emit_console_event(console_tx, ConsoleEvent::CrabUpdated { crab });
            }
        }
    }

    Ok(assignments)
}

async fn get_status(State(state): State<AppState>) -> Result<Json<StatusSnapshot>, ApiError> {
    let db = state.db.lock().await;
    Ok(Json(build_status_snapshot(&db)?))
}

fn build_status_snapshot(conn: &Connection) -> Result<StatusSnapshot, ApiError> {
    let colonies = query_colonies(conn)?;
    let crabs = query_crabs(conn)?;
    let missions = query_missions(conn)?;
    let tasks = query_tasks(conn)?;
    let runs = query_runs(conn)?;

    let completed_runs =
        runs.iter().filter(|run| run.status == RunStatus::Completed).collect::<Vec<_>>();

    let avg_end_to_end_ms = if completed_runs.is_empty() {
        None
    } else {
        let sum: u64 =
            completed_runs.iter().map(|run| run.metrics.end_to_end_ms.unwrap_or_default()).sum();
        Some(sum / completed_runs.len() as u64)
    };

    let summary = StatusSummary {
        total_crabs: crabs.len(),
        busy_crabs: crabs.iter().filter(|crab| matches!(crab.state, CrabState::Busy)).count(),
        running_tasks: tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Running))
            .count(),
        running_runs: runs.iter().filter(|run| matches!(run.status, RunStatus::Running)).count(),
        completed_runs: runs
            .iter()
            .filter(|run| matches!(run.status, RunStatus::Completed))
            .count(),
        failed_runs: runs.iter().filter(|run| matches!(run.status, RunStatus::Failed)).count(),
        total_tokens: runs.iter().map(|run| u64::from(run.metrics.total_tokens)).sum(),
        avg_end_to_end_ms,
    };

    Ok(StatusSnapshot {
        generated_at_ms: now_ms(),
        summary,
        colonies,
        crabs,
        missions,
        tasks,
        runs,
    })
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

fn query_colonies(conn: &Connection) -> Result<Vec<ColonyRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT colony_id, name, description, repo, created_at_ms FROM colonies ORDER BY created_at_ms DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ColonyRecord {
            colony_id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            repo: row.get(3)?,
            created_at_ms: row.get::<_, i64>(4)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_crabs(conn: &Connection) -> Result<Vec<CrabRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT crab_id, colony_id, name, role, state, current_task_id, current_run_id, updated_at_ms FROM crabs ORDER BY crab_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CrabRecord {
            crab_id: row.get(0)?,
            colony_id: row.get(1)?,
            name: row.get(2)?,
            role: row.get(3)?,
            state: CrabState::from_str(&row.get::<_, String>(4)?),
            current_task_id: row.get(5)?,
            current_run_id: row.get(6)?,
            updated_at_ms: row.get::<_, i64>(7)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_missions(conn: &Connection) -> Result<Vec<MissionRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, colony_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions ORDER BY created_at_ms DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MissionRecord {
            mission_id: row.get(0)?,
            colony_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn fetch_mission(conn: &Connection, mission_id: &str) -> Result<Option<MissionRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, colony_id, prompt, workflow_name, status, worktree_path, queue_position, github_issue_number, github_pr_number, created_at_ms FROM missions WHERE mission_id = ?1",
    )?;
    let mut rows = stmt.query(params![mission_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(MissionRecord {
            mission_id: row.get(0)?,
            colony_id: row.get(1)?,
            prompt: row.get(2)?,
            workflow_name: row.get(3)?,
            status: mission_status_from_db(&row.get::<_, String>(4)?),
            worktree_path: row.get(5)?,
            queue_position: row.get(6)?,
            github_issue_number: row.get(7)?,
            github_pr_number: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
        }));
    }
    Ok(None)
}

fn query_tasks(conn: &Connection) -> Result<Vec<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status,
               step_id, role, prompt, context,
               created_at_ms, updated_at_ms
        FROM tasks
        ORDER BY updated_at_ms DESC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TaskRecord {
            task_id: row.get(0)?,
            mission_id: row.get(1)?,
            title: row.get(2)?,
            assigned_crab_id: row.get(3)?,
            status: task_status_from_db(&row.get::<_, String>(4)?),
            step_id: row.get(5)?,
            role: row.get(6)?,
            prompt: row.get(7)?,
            context: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
            updated_at_ms: row.get::<_, i64>(10)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_runs(conn: &Connection) -> Result<Vec<RunRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
               progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
               first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
               started_at_ms, updated_at_ms, completed_at_ms
        FROM runs
        ORDER BY updated_at_ms DESC
        ",
    )?;
    let rows = stmt.query_map([], map_run_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn fetch_crab(conn: &Connection, crab_id: &str) -> Result<Option<CrabRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT crab_id, colony_id, name, role, state, current_task_id, current_run_id, updated_at_ms
        FROM crabs WHERE crab_id = ?1
        ",
    )?;

    let mut rows = stmt.query(params![crab_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(CrabRecord {
            crab_id: row.get(0)?,
            colony_id: row.get(1)?,
            name: row.get(2)?,
            role: row.get(3)?,
            state: CrabState::from_str(&row.get::<_, String>(4)?),
            current_task_id: row.get(5)?,
            current_run_id: row.get(6)?,
            updated_at_ms: row.get::<_, i64>(7)? as u64,
        }));
    }
    Ok(None)
}

fn fetch_task(conn: &Connection, task_id: &str) -> Result<Option<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status,
               step_id, role, prompt, context,
               created_at_ms, updated_at_ms
        FROM tasks WHERE task_id = ?1
        ",
    )?;

    let mut rows = stmt.query(params![task_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(TaskRecord {
            task_id: row.get(0)?,
            mission_id: row.get(1)?,
            title: row.get(2)?,
            assigned_crab_id: row.get(3)?,
            status: task_status_from_db(&row.get::<_, String>(4)?),
            step_id: row.get(5)?,
            role: row.get(6)?,
            prompt: row.get(7)?,
            context: row.get(8)?,
            created_at_ms: row.get::<_, i64>(9)? as u64,
            updated_at_ms: row.get::<_, i64>(10)? as u64,
        }));
    }
    Ok(None)
}

fn fetch_run(conn: &Connection, run_id: &str) -> Result<Option<RunRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode,
               progress_message, summary, prompt_tokens, completion_tokens, total_tokens,
               first_token_ms, llm_duration_ms, execution_duration_ms, end_to_end_ms,
               started_at_ms, updated_at_ms, completed_at_ms
        FROM runs
        WHERE run_id = ?1
        ",
    )?;
    let mut rows = stmt.query(params![run_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(map_run_row(row)?));
    }
    Ok(None)
}

fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        run_id: row.get(0)?,
        mission_id: row.get(1)?,
        task_id: row.get(2)?,
        crab_id: row.get(3)?,
        status: run_status_from_db(&row.get::<_, String>(4)?),
        burrow_path: row.get(5)?,
        burrow_mode: burrow_mode_from_db(&row.get::<_, String>(6)?),
        progress_message: row.get(7)?,
        summary: row.get(8)?,
        metrics: RunMetrics {
            prompt_tokens: row.get::<_, i64>(9)? as u32,
            completion_tokens: row.get::<_, i64>(10)? as u32,
            total_tokens: row.get::<_, i64>(11)? as u32,
            first_token_ms: row.get::<_, Option<i64>>(12)?.map(|v| v as u64),
            llm_duration_ms: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
            execution_duration_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
            end_to_end_ms: row.get::<_, Option<i64>>(15)?.map(|v| v as u64),
        },
        started_at_ms: row.get::<_, i64>(16)? as u64,
        updated_at_ms: row.get::<_, i64>(17)? as u64,
        completed_at_ms: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
    })
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn merge_metrics(
    base: RunMetrics,
    usage_patch: Option<TokenUsagePatch>,
    timing_patch: Option<TimingPatch>,
) -> RunMetrics {
    let mut merged = base;
    if let Some(usage) = usage_patch {
        if let Some(v) = usage.prompt_tokens {
            merged.prompt_tokens = v;
        }
        if let Some(v) = usage.completion_tokens {
            merged.completion_tokens = v;
        }
        merged.total_tokens = usage
            .total_tokens
            .unwrap_or_else(|| merged.prompt_tokens.saturating_add(merged.completion_tokens));
    }
    if let Some(timing) = timing_patch {
        if timing.first_token_ms.is_some() {
            merged.first_token_ms = timing.first_token_ms;
        }
        if timing.llm_duration_ms.is_some() {
            merged.llm_duration_ms = timing.llm_duration_ms;
        }
        if timing.execution_duration_ms.is_some() {
            merged.execution_duration_ms = timing.execution_duration_ms;
        }
        if timing.end_to_end_ms.is_some() {
            merged.end_to_end_ms = timing.end_to_end_ms;
        }
    }
    merged
}

fn task_status_to_db(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Assigned => "assigned",
        TaskStatus::Running => "running",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
    }
}

fn task_status_from_db(raw: &str) -> TaskStatus {
    match raw {
        "assigned" => TaskStatus::Assigned,
        "running" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        "skipped" => TaskStatus::Skipped,
        _ => TaskStatus::Queued,
    }
}

fn mission_status_to_db(status: MissionStatus) -> &'static str {
    match status {
        MissionStatus::Pending => "pending",
        MissionStatus::Running => "running",
        MissionStatus::Completed => "completed",
        MissionStatus::Failed => "failed",
    }
}

fn mission_status_from_db(raw: &str) -> MissionStatus {
    match raw {
        "running" => MissionStatus::Running,
        "completed" => MissionStatus::Completed,
        "failed" => MissionStatus::Failed,
        _ => MissionStatus::Pending,
    }
}

fn run_status_to_db(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Blocked => "blocked",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
    }
}

fn run_status_from_db(raw: &str) -> RunStatus {
    match raw {
        "running" => RunStatus::Running,
        "blocked" => RunStatus::Blocked,
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        _ => RunStatus::Queued,
    }
}

fn burrow_mode_to_db(mode: BurrowMode) -> &'static str {
    match mode {
        BurrowMode::Worktree => "worktree",
        BurrowMode::ExternalRepo => "external_repo",
    }
}

fn burrow_mode_from_db(raw: &str) -> BurrowMode {
    match raw {
        "external_repo" => BurrowMode::ExternalRepo,
        _ => BurrowMode::Worktree,
    }
}

// ---------------------------------------------------------------------------
// Merge-wait background poller
// ---------------------------------------------------------------------------

async fn spawn_merge_wait_poller(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(e) = poll_merge_wait_tasks(&state).await {
            tracing::warn!(err = ?e, "merge-wait poll error");
        }
    }
}

struct MergeWaitPollItem {
    task_id: String,
    mission_id: String,
    pr_number: Option<i64>,
    repo: Option<String>,
}

async fn poll_merge_wait_tasks(state: &AppState) -> Result<(), ApiError> {
    // Find merge-wait tasks that are queued
    let tasks_to_poll: Vec<MergeWaitPollItem> = {
        let db = state.db.lock().await;
        let mut stmt = db.prepare(
            "
            SELECT t.task_id, t.mission_id, m.github_pr_number, c.repo
            FROM tasks t
            JOIN missions m ON t.mission_id = m.mission_id
            JOIN colonies c ON m.colony_id = c.colony_id
            WHERE t.step_id = 'merge-wait' AND t.status = 'queued'
            ",
        )?;
        let rows: Vec<_> = stmt
            .query_map([], |row| {
                Ok(MergeWaitPollItem {
                    task_id: row.get(0)?,
                    mission_id: row.get(1)?,
                    pr_number: row.get(2)?,
                    repo: row.get(3)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        rows
    };

    for item in tasks_to_poll {
        let (Some(pr_num), Some(ref repo)) = (item.pr_number, item.repo) else {
            continue;
        };

        let pr_status = match state.github.get_pr_status(repo, pr_num).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(pr = pr_num, err = ?e, "failed to check PR status");
                continue;
            }
        };

        let assignments = {
            let mut db = state.db.lock().await;
            let tx = db.transaction().map_err(ApiError::from)?;
            let now = now_ms();

            if pr_status.state == "MERGED" || pr_status.merged_at.is_some() {
                let run_id = crabitat_core::RunId::new().to_string();
                tx.execute(
                    "INSERT INTO runs (run_id, mission_id, task_id, crab_id, status, burrow_path, burrow_mode, progress_message, summary, prompt_tokens, completion_tokens, total_tokens, started_at_ms, updated_at_ms, completed_at_ms) VALUES (?1, ?2, ?3, 'system', 'completed', '', 'worktree', 'PR merged', ?4, 0, 0, 0, ?5, ?5, ?5)",
                    params![run_id, item.mission_id, item.task_id, format!("PR #{pr_num} merged"), now],
                )?;

                tx.execute(
                    "UPDATE tasks SET status = 'completed', updated_at_ms = ?2 WHERE task_id = ?1",
                    params![item.task_id, now],
                )?;

                if let Ok(Some(task)) = fetch_task(&tx, &item.task_id) {
                    emit_console_event(&state.console_tx, ConsoleEvent::TaskUpdated { task });
                }

                cascade_workflow(
                    &tx,
                    &item.mission_id,
                    &item.task_id,
                    &state.console_tx,
                    &state.workflows,
                )?;

                let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
                tx.commit().map_err(ApiError::from)?;

                info!(pr = pr_num, mission_id = %item.mission_id, "merge-wait completed: PR merged");
                assignments
            } else if pr_status.state == "CLOSED" {
                tx.execute(
                    "UPDATE tasks SET status = 'failed', updated_at_ms = ?2 WHERE task_id = ?1",
                    params![item.task_id, now],
                )?;

                if let Ok(Some(task)) = fetch_task(&tx, &item.task_id) {
                    emit_console_event(&state.console_tx, ConsoleEvent::TaskUpdated { task });
                }

                cascade_workflow(
                    &tx,
                    &item.mission_id,
                    &item.task_id,
                    &state.console_tx,
                    &state.workflows,
                )?;

                let assignments = run_scheduler_tick_db(&tx, &state.console_tx)?;
                tx.commit().map_err(ApiError::from)?;

                info!(pr = pr_num, mission_id = %item.mission_id, "merge-wait failed: PR closed without merge");
                assignments
            } else {
                continue;
            }
        };

        dispatch_assignments(state, assignments).await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        apply_schema(&conn).unwrap();
        let (console_tx, _) = broadcast::channel::<String>(256);
        let workflows = WorkflowRegistry {
            manifests: HashMap::new(),
            prompts_path: PathBuf::from("/tmp/test-prompts"),
        };
        AppState {
            db: Arc::new(Mutex::new(conn)),
            crab_channels: Arc::new(Mutex::new(HashMap::new())),
            console_tx,
            workflows: Arc::new(workflows),
            github: GitHubClient { http: reqwest::Client::new(), token: None },
        }
    }

    async fn setup_colony(state: &AppState) -> ColonyRecord {
        create_colony(
            State(state.clone()),
            Json(CreateColonyRequest { name: "test-colony".into(), description: None, repo: None }),
        )
        .await
        .unwrap()
        .0
    }

    #[tokio::test]
    async fn register_and_list_crabs() {
        let state = test_state();
        let colony = setup_colony(&state).await;

        let crab = register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                colony_id: colony.colony_id.clone(),
                name: "Alice".into(),
                role: "coder".into(),
                state: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(crab.crab_id, "crab-1");
        assert_eq!(crab.name, "Alice");
        assert_eq!(crab.colony_id, colony.colony_id);
        assert!(matches!(crab.state, CrabState::Idle));

        let crabs = list_crabs(State(state.clone())).await.unwrap().0;
        assert_eq!(crabs.len(), 1);
    }

    #[tokio::test]
    async fn create_mission_and_task() {
        let state = test_state();
        let colony = setup_colony(&state).await;

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                colony_id: colony.colony_id.clone(),
                prompt: "Implement feature X".into(),
                workflow: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(!mission.mission_id.is_empty());
        assert_eq!(mission.colony_id, colony.colony_id);

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                colony_id: colony.colony_id.clone(),
                name: "Alice".into(),
                role: "coder".into(),
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
        let colony = setup_colony(&state).await;

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                colony_id: colony.colony_id.clone(),
                name: "Alice".into(),
                role: "coder".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                colony_id: colony.colony_id.clone(),
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
        let colony = setup_colony(&state).await;

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-1".into(),
                colony_id: colony.colony_id.clone(),
                name: "Alice".into(),
                role: "coder".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        register_crab(
            State(state.clone()),
            Json(RegisterCrabRequest {
                crab_id: "crab-2".into(),
                colony_id: colony.colony_id.clone(),
                name: "Bob".into(),
                role: "reviewer".into(),
                state: None,
            }),
        )
        .await
        .unwrap();

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                colony_id: colony.colony_id.clone(),
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
        assert_eq!(snapshot.colonies.len(), 1);
    }

    #[tokio::test]
    async fn get_mission_by_id() {
        let state = test_state();
        let colony = setup_colony(&state).await;

        let mission = create_mission(
            State(state.clone()),
            Json(CreateMissionRequest {
                colony_id: colony.colony_id.clone(),
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
        assert_eq!(fetched.colony_id, colony.colony_id);
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
