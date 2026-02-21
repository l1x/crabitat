use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColonyId(pub Uuid);

impl ColonyId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ColonyId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ColonyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MissionId(pub Uuid);

impl MissionId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MissionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MissionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub Uuid);

impl TaskId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub Uuid);

impl RunId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColonyRole {
    Chief,
    Crab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BurrowMode {
    Worktree,
    ExternalRepo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Assigned,
    Running,
    Blocked,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Blocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colony {
    pub id: ColonyId,
    pub name: String,
    pub description: String,
    pub created_at_ms: u64,
}

impl Colony {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: ColonyId::new(),
            name: name.into(),
            description: description.into(),
            created_at_ms: now_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub prompt: String,
    pub workflow_name: Option<String>,
    pub status: MissionStatus,
    pub worktree_path: Option<String>,
    pub created_at_ms: u64,
}

impl Mission {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            id: MissionId::new(),
            prompt: prompt.into(),
            workflow_name: None,
            status: MissionStatus::Pending,
            worktree_path: None,
            created_at_ms: now_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub mission_id: MissionId,
    pub title: String,
    pub assigned_crab_id: Option<String>,
    pub status: TaskStatus,
    pub step_id: Option<String>,
    pub role: Option<String>,
    pub prompt: Option<String>,
    pub context: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Workflow manifest types (parsed from TOML)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMeta {
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub role: String,
    pub prompt_file: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub condition: Option<String>,
    #[serde(default)]
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowManifest {
    pub workflow: WorkflowMeta,
    #[serde(rename = "steps")]
    pub steps: Vec<WorkflowStep>,
}

/// Evaluate a simple condition expression like `step_id.field == 'value'`
/// against a context map of `{"step_id.field": "value"}`.
pub fn evaluate_condition(condition: &str, context: &HashMap<String, String>) -> bool {
    // Parse: "step_id.field == 'value'"
    let parts: Vec<&str> = condition.splitn(2, "==").collect();
    if parts.len() != 2 {
        return false;
    }
    let key = parts[0].trim();
    let expected = parts[1].trim().trim_matches('\'').trim_matches('"');
    context.get(key).is_some_and(|v| v == expected)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Burrow {
    pub path: String,
    pub mode: BurrowMode,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunMetrics {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub first_token_ms: Option<u64>,
    pub llm_duration_ms: Option<u64>,
    pub execution_duration_ms: Option<u64>,
    pub end_to_end_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub mission_id: MissionId,
    pub task_id: TaskId,
    pub crab_id: String,
    pub status: RunStatus,
    pub burrow: Burrow,
    pub metrics: RunMetrics,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub completed_at_ms: Option<u64>,
}

#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colony_id_is_unique() {
        let a = ColonyId::new();
        let b = ColonyId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn mission_id_is_unique() {
        let a = MissionId::new();
        let b = MissionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn hello_world() {
        assert_eq!(1 + 1, 2);
    }
}
