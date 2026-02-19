use crabitat_core::{MissionId, RunId, RunMetrics, RunStatus, TaskId, TaskStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub message_id: Uuid,
    pub mission_id: Option<MissionId>,
    pub task_id: Option<TaskId>,
    pub run_id: Option<RunId>,
    pub from: String,
    pub to: String,
    pub kind: MessageKind,
    pub sent_at_ms: u64,
}

impl Envelope {
    #[must_use]
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: MessageKind,
        sent_at_ms: u64,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            mission_id: None,
            task_id: None,
            run_id: None,
            from: from.into(),
            to: to.into(),
            kind,
            sent_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum MessageKind {
    TaskAssigned(TaskAssigned),
    TaskProgress(TaskProgress),
    RunUpdate(RunUpdate),
    RunComplete(RunComplete),
    Heartbeat(Heartbeat),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssigned {
    pub task_id: TaskId,
    pub mission_id: MissionId,
    pub title: String,
    pub mission_prompt: String,
    pub desired_status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunUpdate {
    pub run_id: RunId,
    pub status: RunStatus,
    pub note: String,
    pub metrics: RunMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunComplete {
    pub run_id: RunId,
    pub status: RunStatus,
    pub summary: String,
    pub metrics: RunMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub crab_id: String,
    pub healthy: bool,
}
