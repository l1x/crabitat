# PRD: Workflow Engine for Crabitat

## 1. Problem

Crabitat has a flat task model: missions contain tasks, but tasks have no dependencies, no role requirements, no condition logic, and no context forwarding. Every multi-step workflow (plan, implement, review, fix, PR) must be manually orchestrated by a human or a chief agent making ad-hoc API calls.

This is fragile, unrepeatable, and prevents scaling to multiple concurrent missions.

## 2. Objective

Add a **declarative workflow engine** that:

- Reads workflow manifests (TOML) from an external `agent-prompts` repo
- Expands a mission into a dependency DAG of tasks
- Schedules tasks to crabs by role
- Evaluates conditions between steps (e.g. review PASS/FAIL branching)
- Forwards context from completed steps to downstream steps
- Manages the fix/review retry loop automatically
- Tracks mission-level status (Pending/Running/Completed/Failed)

## 3. Key Concepts

| Concept | Description |
|---------|-------------|
| **Workflow Manifest** | A TOML file defining a named sequence of steps with dependencies and conditions |
| **Step** | One unit of work in a workflow: has an id, role, prompt file, dependencies, and optional condition |
| **Condition** | Simple expression (`step.field == 'value'`) evaluated against prior step results |
| **Context forwarding** | Run summaries from completed steps are accumulated and injected into downstream step prompts |
| **Role matching** | Each step declares a required role; the scheduler matches it to idle crabs with that role |
| **Worktree isolation** | 1 mission = 1 worktree at `burrows/mission-<id>/`. Steps within a mission share that worktree sequentially |

## 4. Workflow Manifest Format

```toml
[workflow]
name = "dev-task"
description = "Plan, implement, review, fix, and PR"
version = "1.0.0"

[[steps]]
id = "plan"
role = "planner"
prompt_file = "do/plan.md"

[[steps]]
id = "implement"
role = "worker"
prompt_file = "do/implement.md"
depends_on = ["plan"]

[[steps]]
id = "review"
role = "reviewer"
prompt_file = "do/review.md"
depends_on = ["implement"]

[[steps]]
id = "fix"
role = "worker"
prompt_file = "do/implement.md"
depends_on = ["review"]
condition = "review.result == 'FAIL'"
max_retries = 3

[[steps]]
id = "pr"
role = "any"
prompt_file = "do/pr.md"
depends_on = ["review"]
condition = "review.result == 'PASS'"
```

Step prompt files use template variables: `{{mission_prompt}}`, `{{context}}`, `{{worktree_path}}`.

## 5. Execution Model

### 5.1 Mission Creation

```
POST /v1/missions
{
  "colony_id": "...",
  "prompt": "Fix issue #6",
  "workflow": "dev-task"
}
```

The control-plane:
1. Creates the mission row
2. Looks up the workflow manifest in the registry
3. Calls `expand_workflow_into_tasks()` to create one task per step
4. Inserts dependency edges into `task_deps`
5. Sets tasks with no deps to `Queued`, others to `Blocked`
6. Sets mission worktree path to `burrows/mission-<id>/`

### 5.2 Scheduling

The scheduler (`POST /v1/scheduler/tick`) runs periodically or on demand:
1. Queries all `Queued` tasks ordered by creation time
2. Queries all `idle` crabs
3. For each queued task:
   - Skips if another task in the same mission is `Running` (worktree safety)
   - Matches task role to crab role (`any` matches anything)
   - Assigns the task, marks the crab busy, sends `TaskAssigned` via WebSocket

### 5.3 Cascade

When a run completes (`POST /v1/runs/complete`), `cascade_workflow()` fires:
1. Finds all tasks that depend on the completed task
2. For each dependent, checks if **all** its dependencies are terminal
3. Evaluates the dependent's condition against the context map
4. If condition met: sets status to `Queued`, injects accumulated context
5. If condition not met: sets status to `Skipped`, recurses to cascade further
6. If the completed task failed: cascades failure to all dependents
7. When all tasks are terminal: updates mission status to Completed/Failed

### 5.4 Fix/Review Retry Loop

When the `fix` step completes, the engine re-queues the `review` step. This creates a cycle bounded by `max_retries`. Each iteration, the reviewer sees fresh context from the latest fix.

## 6. API Changes

| Method | Route | Purpose |
|--------|-------|---------|
| GET | `/v1/workflows` | List available workflow names |
| POST | `/v1/scheduler/tick` | Manually trigger the scheduler |
| POST | `/v1/missions` | Now accepts optional `workflow` field |

## 7. Schema Changes

### Missions table (new columns)

- `workflow_name TEXT` -- which workflow was used
- `status TEXT NOT NULL DEFAULT 'pending'` -- mission lifecycle
- `worktree_path TEXT` -- shared worktree for all steps

### Tasks table (new columns)

- `step_id TEXT` -- links to workflow step id
- `role TEXT` -- required crab role
- `prompt TEXT` -- rendered prompt for this step
- `context TEXT` -- accumulated context from prior steps

### New table: task_deps

```sql
CREATE TABLE task_deps (
  task_id TEXT NOT NULL,
  depends_on_task_id TEXT NOT NULL,
  PRIMARY KEY (task_id, depends_on_task_id)
);
```

## 8. Task Status Model

```
Queued --> Assigned --> Running --> Completed
                                --> Failed
Blocked --> Queued (when deps met + condition passes)
        --> Skipped (when condition fails)
        --> Failed (when a dependency fails)
```

New status: `Skipped` -- the task's condition was not met, so it was bypassed.

## 9. Success Criteria

1. `GET /v1/workflows` returns loaded workflow names
2. Creating a mission with `workflow: "dev-task"` produces 5 tasks with correct dependencies
3. Only the `plan` task starts as `Queued`; others start as `Blocked`
4. Scheduler assigns `plan` to an idle planner crab
5. Completing plan cascades: `implement` becomes `Queued`
6. Completing implement cascades: `review` becomes `Queued`
7. Review with `result=PASS` cascades: `pr` becomes `Queued`, `fix` becomes `Skipped`
8. Review with `result=FAIL` cascades: `fix` becomes `Queued`, `pr` stays `Blocked`
9. Completing fix re-queues review (retry loop)
10. Full lifecycle ends with mission status `Completed`

## 10. Non-Goals (v1)

- Parallel steps within a mission (worktree conflict)
- Dynamic workflow modification at runtime
- Workflow versioning/migration
- UI for workflow editing (TOML files are the source of truth)
- Automatic worktree creation (done by the crab at run time)
