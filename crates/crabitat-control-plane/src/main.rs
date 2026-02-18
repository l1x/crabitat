use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use crabitat_core::{
    BurrowMode, Mission, RunId, RunMetrics, RunStatus, TaskId, TaskStatus, now_ms,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;
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
    },
}

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
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

#[derive(Debug, Serialize)]
struct CrabRecord {
    crab_id: String,
    name: String,
    role: String,
    state: CrabState,
    current_task_id: Option<String>,
    current_run_id: Option<String>,
    updated_at_ms: u64,
}

#[derive(Debug, Serialize)]
struct MissionRecord {
    mission_id: String,
    prompt: String,
    created_at_ms: u64,
}

#[derive(Debug, Serialize)]
struct TaskRecord {
    task_id: String,
    mission_id: String,
    title: String,
    assigned_crab_id: Option<String>,
    status: TaskStatus,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
struct StatusSnapshot {
    generated_at_ms: u64,
    summary: StatusSummary,
    crabs: Vec<CrabRecord>,
    missions: Vec<MissionRecord>,
    tasks: Vec<TaskRecord>,
    runs: Vec<RunRecord>,
}

#[derive(Debug, Deserialize)]
struct RegisterCrabRequest {
    crab_id: String,
    name: String,
    role: String,
    state: Option<CrabState>,
}

#[derive(Debug, Deserialize)]
struct CreateMissionRequest {
    prompt: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match Cli::parse().command {
        Command::Serve { port, db_path } => serve(port, &db_path).await?,
    }

    Ok(())
}

async fn serve(port: u16, db_path: &Path) -> Result<()> {
    let connection = init_db(db_path)?;
    let state = AppState { db: Arc::new(Mutex::new(connection)) };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/crabs", get(list_crabs))
        .route("/v1/crabs/register", post(register_crab))
        .route("/v1/missions", post(create_mission).get(list_missions))
        .route("/v1/tasks", post(create_task).get(list_tasks))
        .route("/v1/runs/start", post(start_run))
        .route("/v1/runs/update", post(update_run))
        .route("/v1/runs/complete", post(complete_run))
        .route("/v1/status", get(get_status))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("control-plane listening on http://{}", addr);
    info!("sqlite database at {}", db_path.display());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn init_db(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS crabs (
          crab_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          role TEXT NOT NULL,
          state TEXT NOT NULL,
          current_task_id TEXT,
          current_run_id TEXT,
          updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS missions (
          mission_id TEXT PRIMARY KEY,
          prompt TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
          task_id TEXT PRIMARY KEY,
          mission_id TEXT NOT NULL,
          title TEXT NOT NULL,
          assigned_crab_id TEXT,
          status TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(mission_id) REFERENCES missions(mission_id)
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
    Ok(conn)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn register_crab(
    State(state): State<AppState>,
    Json(request): Json<RegisterCrabRequest>,
) -> Result<Json<CrabRecord>, ApiError> {
    if request.crab_id.trim().is_empty()
        || request.name.trim().is_empty()
        || request.role.trim().is_empty()
    {
        return Err(ApiError::bad_request("crab_id, name, and role are required"));
    }

    let updated_at_ms = now_ms();
    let crab_state = request.state.unwrap_or(CrabState::Idle);

    let db = state.db.lock().await;
    db.execute(
        "
        INSERT INTO crabs (crab_id, name, role, state, current_task_id, current_run_id, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)
        ON CONFLICT(crab_id) DO UPDATE SET
          name=excluded.name,
          role=excluded.role,
          state=excluded.state,
          updated_at_ms=excluded.updated_at_ms
        ",
        params![request.crab_id, request.name, request.role, crab_state.as_str(), updated_at_ms],
    )?;

    let crab = fetch_crab(&db, &request.crab_id)?
        .ok_or_else(|| ApiError::internal("failed to reload crab after registration"))?;
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

    let mission = Mission::new(request.prompt);
    let row = MissionRecord {
        mission_id: mission.id.to_string(),
        prompt: mission.prompt,
        created_at_ms: mission.created_at_ms,
    };

    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO missions (mission_id, prompt, created_at_ms) VALUES (?1, ?2, ?3)",
        params![row.mission_id, row.prompt, row.created_at_ms],
    )?;

    Ok(Json(row))
}

async fn list_missions(
    State(state): State<AppState>,
) -> Result<Json<Vec<MissionRecord>>, ApiError> {
    let db = state.db.lock().await;
    let missions = query_missions(&db)?;
    Ok(Json(missions))
}

async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    if request.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    let db = state.db.lock().await;

    let mission_exists: i64 = db.query_row(
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

    db.execute(
        "
        INSERT INTO tasks (task_id, mission_id, title, assigned_crab_id, status, created_at_ms, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ",
        params![
            task_id,
            request.mission_id,
            request.title,
            request.assigned_crab_id,
            task_status_to_db(status),
            created_at_ms,
            created_at_ms
        ],
    )?;

    if let Some(crab_id) = request.assigned_crab_id {
        db.execute(
            "UPDATE crabs SET state = 'busy', current_task_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?1",
            params![crab_id, task_id, created_at_ms],
        )?;
    }

    let task = fetch_task(&db, &task_id)?
        .ok_or_else(|| ApiError::internal("failed to reload task after creation"))?;
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

    let db = state.db.lock().await;

    let mission_exists: i64 = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM missions WHERE mission_id = ?1)",
        params![request.mission_id],
        |row| row.get(0),
    )?;
    if mission_exists == 0 {
        return Err(ApiError::not_found("mission_id not found"));
    }

    let task_exists: i64 = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE task_id = ?1)",
        params![request.task_id],
        |row| row.get(0),
    )?;
    if task_exists == 0 {
        return Err(ApiError::not_found("task_id not found"));
    }

    db.execute(
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

    db.execute(
        "UPDATE tasks SET assigned_crab_id = ?1, status = ?2, updated_at_ms = ?3 WHERE task_id = ?4",
        params![request.crab_id, task_status_to_db(TaskStatus::Running), now, request.task_id],
    )?;

    db.execute(
        "UPDATE crabs SET state = 'busy', current_task_id = ?1, current_run_id = ?2, updated_at_ms = ?3 WHERE crab_id = ?4",
        params![request.task_id, run_id, now, request.crab_id],
    )?;

    let run = fetch_run(&db, &run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after start"))?;
    Ok(Json(run))
}

async fn update_run(
    State(state): State<AppState>,
    Json(request): Json<UpdateRunRequest>,
) -> Result<Json<RunRecord>, ApiError> {
    let db = state.db.lock().await;
    let existing =
        fetch_run(&db, &request.run_id)?.ok_or_else(|| ApiError::not_found("run_id not found"))?;

    let now = now_ms();
    let status = request.status.unwrap_or(existing.status);
    let progress_message = request.progress_message.unwrap_or(existing.progress_message.clone());
    let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

    db.execute(
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
            db.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Running), now],
            )?;
            db.execute(
                "UPDATE crabs SET state = 'busy', current_task_id = ?2, current_run_id = ?3, updated_at_ms = ?4 WHERE crab_id = ?1",
                params![existing.crab_id, existing.task_id, existing.run_id, now],
            )?;
        }
        RunStatus::Blocked => {
            db.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Blocked), now],
            )?;
        }
        RunStatus::Completed => {
            db.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Completed), now],
            )?;
        }
        RunStatus::Failed => {
            db.execute(
                "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
                params![existing.task_id, task_status_to_db(TaskStatus::Failed), now],
            )?;
        }
        RunStatus::Queued => {}
    }

    let updated = fetch_run(&db, &request.run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after update"))?;
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

    let db = state.db.lock().await;
    let existing =
        fetch_run(&db, &request.run_id)?.ok_or_else(|| ApiError::not_found("run_id not found"))?;

    let completed_at = now_ms();
    let metrics = merge_metrics(existing.metrics.clone(), request.token_usage, request.timing);

    db.execute(
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
    db.execute(
        "UPDATE tasks SET status = ?2, updated_at_ms = ?3 WHERE task_id = ?1",
        params![existing.task_id, task_status_to_db(task_status), completed_at],
    )?;

    db.execute(
        "UPDATE crabs SET state = 'idle', current_task_id = NULL, current_run_id = NULL, updated_at_ms = ?2 WHERE crab_id = ?1",
        params![existing.crab_id, completed_at],
    )?;

    let run = fetch_run(&db, &request.run_id)?
        .ok_or_else(|| ApiError::internal("failed to reload run after completion"))?;
    Ok(Json(run))
}

async fn get_status(State(state): State<AppState>) -> Result<Json<StatusSnapshot>, ApiError> {
    let db = state.db.lock().await;
    let crabs = query_crabs(&db)?;
    let missions = query_missions(&db)?;
    let tasks = query_tasks(&db)?;
    let runs = query_runs(&db)?;

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

    Ok(Json(StatusSnapshot { generated_at_ms: now_ms(), summary, crabs, missions, tasks, runs }))
}

fn query_crabs(conn: &Connection) -> Result<Vec<CrabRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT crab_id, name, role, state, current_task_id, current_run_id, updated_at_ms FROM crabs ORDER BY crab_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CrabRecord {
            crab_id: row.get(0)?,
            name: row.get(1)?,
            role: row.get(2)?,
            state: CrabState::from_str(&row.get::<_, String>(3)?),
            current_task_id: row.get(4)?,
            current_run_id: row.get(5)?,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_missions(conn: &Connection) -> Result<Vec<MissionRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT mission_id, prompt, created_at_ms FROM missions ORDER BY created_at_ms DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MissionRecord {
            mission_id: row.get(0)?,
            prompt: row.get(1)?,
            created_at_ms: row.get::<_, i64>(2)? as u64,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_tasks(conn: &Connection) -> Result<Vec<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status, created_at_ms, updated_at_ms
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
            created_at_ms: row.get::<_, i64>(5)? as u64,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
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
        SELECT crab_id, name, role, state, current_task_id, current_run_id, updated_at_ms
        FROM crabs WHERE crab_id = ?1
        ",
    )?;

    let mut rows = stmt.query(params![crab_id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(CrabRecord {
            crab_id: row.get(0)?,
            name: row.get(1)?,
            role: row.get(2)?,
            state: CrabState::from_str(&row.get::<_, String>(3)?),
            current_task_id: row.get(4)?,
            current_run_id: row.get(5)?,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
        }));
    }
    Ok(None)
}

fn fetch_task(conn: &Connection, task_id: &str) -> Result<Option<TaskRecord>, ApiError> {
    let mut stmt = conn.prepare(
        "
        SELECT task_id, mission_id, title, assigned_crab_id, status, created_at_ms, updated_at_ms
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
            created_at_ms: row.get::<_, i64>(5)? as u64,
            updated_at_ms: row.get::<_, i64>(6)? as u64,
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
    }
}

fn task_status_from_db(raw: &str) -> TaskStatus {
    match raw {
        "assigned" => TaskStatus::Assigned,
        "running" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        _ => TaskStatus::Queued,
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
