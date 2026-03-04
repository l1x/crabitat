use crate::github::GitHubClient;
use crate::workflows::WorkflowRegistry;
use crabitat_core::{BurrowMode, MissionStatus, RunMetrics, RunStatus, TaskStatus};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

/// Issues cache TTL: 5 minutes.
pub(crate) const ISSUES_CACHE_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Arc<Mutex<Connection>>,
    pub(crate) workflows: Arc<RwLock<WorkflowRegistry>>,
    pub(crate) github: GitHubClient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CrabState {
    Idle,
    Busy,
    Offline,
}

impl CrabState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Offline => "offline",
        }
    }

    pub(crate) fn from_str(raw: &str) -> Self {
        match raw {
            "busy" => Self::Busy,
            "offline" => Self::Offline,
            _ => Self::Idle,
        }
    }
}

// ---------------------------------------------------------------------------
// Record types (API responses)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepoRecord {
    pub(crate) repo_id: String,
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) full_name: String,
    pub(crate) default_branch: String,
    pub(crate) language: String,
    pub(crate) local_path: String,
    pub(crate) stacks: Vec<String>,
    pub(crate) created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrabRecord {
    pub(crate) crab_id: String,
    pub(crate) repo_id: String,
    pub(crate) name: String,
    pub(crate) state: CrabState,
    pub(crate) current_task_id: Option<String>,
    pub(crate) current_run_id: Option<String>,
    pub(crate) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MissionRecord {
    pub(crate) mission_id: String,
    pub(crate) repo_id: String,
    pub(crate) prompt: String,
    pub(crate) workflow_name: Option<String>,
    pub(crate) status: MissionStatus,
    pub(crate) worktree_path: Option<String>,
    pub(crate) queue_position: Option<i64>,
    pub(crate) github_issue_number: Option<i64>,
    pub(crate) github_pr_number: Option<i64>,
    pub(crate) created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskRecord {
    pub(crate) task_id: String,
    pub(crate) mission_id: String,
    pub(crate) title: String,
    pub(crate) assigned_crab_id: Option<String>,
    pub(crate) status: TaskStatus,
    pub(crate) step_id: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) context: Option<String>,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunRecord {
    pub(crate) run_id: String,
    pub(crate) mission_id: String,
    pub(crate) task_id: String,
    pub(crate) crab_id: String,
    pub(crate) status: RunStatus,
    pub(crate) burrow_path: String,
    pub(crate) burrow_mode: BurrowMode,
    pub(crate) progress_message: String,
    pub(crate) summary: Option<String>,
    pub(crate) metrics: RunMetrics,
    pub(crate) started_at_ms: u64,
    pub(crate) updated_at_ms: u64,
    pub(crate) completed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusSummary {
    pub(crate) total_crabs: usize,
    pub(crate) busy_crabs: usize,
    pub(crate) running_tasks: usize,
    pub(crate) running_runs: usize,
    pub(crate) completed_runs: usize,
    pub(crate) failed_runs: usize,
    pub(crate) total_tokens: u64,
    pub(crate) avg_end_to_end_ms: Option<u64>,
    pub(crate) cached_issue_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusSnapshot {
    pub(crate) generated_at_ms: u64,
    pub(crate) summary: StatusSummary,
    pub(crate) repos: Vec<RepoRecord>,
    pub(crate) crabs: Vec<CrabRecord>,
    pub(crate) missions: Vec<MissionRecord>,
    pub(crate) tasks: Vec<TaskRecord>,
    pub(crate) runs: Vec<RunRecord>,
    pub(crate) repo_issue_counts: HashMap<String, i64>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct RegisterCrabRequest {
    pub(crate) crab_id: String,
    pub(crate) repo_id: String,
    pub(crate) name: String,
    pub(crate) state: Option<CrabState>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateMissionRequest {
    pub(crate) repo_id: String,
    pub(crate) prompt: String,
    pub(crate) workflow: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTaskRequest {
    pub(crate) mission_id: String,
    pub(crate) title: String,
    pub(crate) assigned_crab_id: Option<String>,
    pub(crate) status: Option<TaskStatus>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StartRunRequest {
    pub(crate) run_id: Option<String>,
    pub(crate) mission_id: String,
    pub(crate) task_id: String,
    pub(crate) crab_id: String,
    pub(crate) burrow_path: String,
    pub(crate) burrow_mode: BurrowMode,
    pub(crate) status: Option<RunStatus>,
    pub(crate) progress_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TokenUsagePatch {
    pub(crate) prompt_tokens: Option<u32>,
    pub(crate) completion_tokens: Option<u32>,
    pub(crate) total_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TimingPatch {
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) llm_duration_ms: Option<u64>,
    pub(crate) execution_duration_ms: Option<u64>,
    pub(crate) end_to_end_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateRunRequest {
    pub(crate) run_id: String,
    pub(crate) status: Option<RunStatus>,
    pub(crate) progress_message: Option<String>,
    pub(crate) token_usage: Option<TokenUsagePatch>,
    pub(crate) timing: Option<TimingPatch>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompleteRunRequest {
    pub(crate) run_id: String,
    pub(crate) status: RunStatus,
    pub(crate) summary: Option<String>,
    pub(crate) token_usage: Option<TokenUsagePatch>,
    pub(crate) timing: Option<TimingPatch>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateRepoRequest {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) default_branch: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) local_path: String,
    pub(crate) stacks: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateRepoRequest {
    pub(crate) default_branch: Option<String>,
    pub(crate) local_path: Option<String>,
    pub(crate) stacks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitHubIssueRecord {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) labels: Vec<String>,
    pub(crate) state: String,
    pub(crate) already_queued: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueueIssueRequest {
    pub(crate) issue_number: i64,
    pub(crate) workflow: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListIssuesQuery {
    #[serde(default)]
    pub(crate) refresh: Option<bool>,
}

// ---------------------------------------------------------------------------
// Workflow DB types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowStepRecord {
    pub(crate) step_id: String,
    pub(crate) prompt_file: String,
    pub(crate) depends_on: Vec<String>,
    pub(crate) condition: Option<String>,
    pub(crate) max_retries: u32,
    pub(crate) position: i64,
    pub(crate) include: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowRecord {
    pub(crate) workflow_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) include: Vec<String>,
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) commit_hash: Option<String>,
    pub(crate) created_at_ms: u64,
    pub(crate) steps: Vec<WorkflowStepRecord>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateWorkflowStepInput {
    pub(crate) step_id: String,
    pub(crate) prompt_file: Option<String>,
    pub(crate) depends_on: Option<Vec<String>>,
    pub(crate) condition: Option<String>,
    pub(crate) max_retries: Option<u32>,
    pub(crate) include: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateWorkflowRequest {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) include: Option<Vec<String>>,
    pub(crate) version: Option<String>,
    pub(crate) steps: Vec<CreateWorkflowStepInput>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateWorkflowRequest {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) include: Option<Vec<String>>,
    pub(crate) version: Option<String>,
    pub(crate) steps: Option<Vec<CreateWorkflowStepInput>>,
}

// ---------------------------------------------------------------------------
// Prompt-file & stack query types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct PromptFileQuery {
    pub(crate) path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PromptFilePreview {
    pub(crate) path: String,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct StackEntry {
    pub(crate) name: String,
    pub(crate) path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillRecord {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crab_state_roundtrip() {
        for state in [CrabState::Idle, CrabState::Busy, CrabState::Offline] {
            let s = state.as_str();
            let back = CrabState::from_str(s);
            assert_eq!(back.as_str(), s);
        }
    }

    #[test]
    fn crab_state_unknown_defaults_idle() {
        let state = CrabState::from_str("xyz");
        assert_eq!(state.as_str(), "idle");
    }

    #[test]
    fn crab_state_serde_roundtrip() {
        let state = CrabState::Busy;
        let json = serde_json::to_string(&state).unwrap();
        let back: CrabState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.as_str(), "busy");
    }

    #[test]
    fn issues_cache_ttl_value() {
        assert_eq!(ISSUES_CACHE_TTL_MS, 300_000);
    }
}
