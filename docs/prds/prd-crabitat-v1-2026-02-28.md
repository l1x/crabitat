# Crabitat — Product Requirements Document

**Version:** 1.0
**Date:** 2026-02-28

---

## Overview

Crabitat is a local-first orchestration platform for autonomous coding agents. It manages a fleet of AI agents ("crabs") that execute multi-step development workflows against GitHub repositories on a single machine. The system takes GitHub issues as input, expands them through configurable workflow pipelines, and dispatches tasks to agents that work in isolated git worktrees.

The platform consists of three components:

1. **Control-Plane** (`crabitat-control-plane`) — the Rust backend that serves the REST API, manages the SQLite database, runs the scheduler, and orchestrates the workflow engine.
2. **Console** (`crabitat-console`) — the Astro SSR web application that provides the operator UI for managing repos, issues, missions, agents, and workflows.
3. **Crab CLI** (`crabitat-crab`) — the Rust agent binary that registers with the control-plane, polls for tasks, executes them in git worktrees using Claude Code, and reports results.

---

## Problem Statement

Running autonomous coding agents at scale on a codebase requires coordination that doesn't exist out of the box. Individual agents can execute prompts, but there is no system for:

- **Sequencing work** — a feature implementation requires planning, coding, QA, and review steps in order, with dependencies and retry logic.
- **Parallelizing across repos** — an organization manages dozens of repositories, each needing its own agent pool and issue queue.
- **Composing prompts** — each repo has a unique tech stack (Rust, Astro, mise, etc.) and the same base workflow needs different stack-specific context appended per repo.
- **Visibility** — knowing which agents are idle, which missions are running, what step they're on, and whether a PR has merged.
- **Autonomy** — agents should work independently once assigned, creating worktrees, running Claude Code, reporting results, and moving on to the next task without human intervention.

Without this, operators manually invoke agents per-issue, copy-paste context, and lose track of what's running where.

---

## Goals

1. **Repos as units of management** — onboard GitHub repositories with local paths and declared tech stacks; fetch and cache their issues automatically.
2. **Declarative workflows** — define multi-step agent workflows in TOML, version them in a prompts repo, and sync them into the system.
3. **Assembled workflows** — automatically combine base workflows with repo-specific stack prompts so each repo gets the right context without manual wiring.
4. **Issue-to-mission pipeline** — queue GitHub issues as missions, expand them into dependent tasks via workflows, and dispatch tasks to available agents.
5. **Autonomous execution** — agents poll for work, execute tasks in isolated git worktrees using Claude Code, and report structured results including token usage.
6. **Workflow cascade** — when a task completes, automatically evaluate conditions, unblock dependents, and schedule the next step.
7. **Operational visibility** — a real-time console showing repos, issues, missions, task pipelines, agent status, and workflow configuration.

---

## Job Stories

1. As an operator, I can onboard a repository with its local path and tech stacks, so that the system knows where to run agents and what prompts to include.
2. As an operator, I can view open GitHub issues for any onboarded repo and queue them as missions with one click, so that work enters the pipeline without manual prompt writing.
3. As an operator, I can sync workflow definitions from a prompts repo, so that the team's workflow changes take effect immediately.
4. As an operator, I can see which agents are idle, busy, or offline and what task each is working on, so that I can diagnose bottlenecks.
5. As an operator, I can see the full pipeline of a mission — which steps are queued, running, blocked, completed, or failed — so that I understand progress at a glance.
6. As an operator, I can select which assembled workflow to use when queuing an issue, so that I can override the default for special cases.
7. As an agent, I can register with the control-plane, poll for assigned tasks, execute them in isolated worktrees, and report results, so that work proceeds autonomously.
8. As an agent, I can receive task prompts with full stack-specific context already assembled, so that I don't need to know about the repo's tech stack myself.

---

## Assumptions

1. The platform runs on a single machine with local filesystem access to all repo clones.
2. SQLite is sufficient for the control-plane database at the expected scale (tens of repos, hundreds of missions).
3. GitHub is the source of truth for issues; no other issue trackers are supported in v1.
4. Agents (crabs) are processes that interact with the control-plane over REST on the same machine.
5. The prompts repo (`agent-prompts`) is a **read-only** git submodule. Crabitat never modifies it — it reads TOML workflows and prompt files, then assembles and stores its own workflow variants in the local database.
6. The `gh` CLI or a `GITHUB_TOKEN` environment variable provides GitHub API access.
7. Claude Code (`claude` CLI) is installed and available on the system PATH for agent task execution.

---

## Functional Requirements

### FR-1: Repository Management

Operators can onboard, update, and remove GitHub repositories.

- FR-1.1: Create a repo by selecting from GitHub (via `gh` CLI), choosing a local directory path, and optionally selecting tech stacks.
  - Acceptance: POST `/v1/repos` creates a repo record with owner, name, default_branch, language, local_path, and stacks. Console reflects the new repo on next page load.

- FR-1.2: View repo details including branch, language, local path, language breakdown bar, stacks, open issues, workflows, and mission pipeline.
  - Acceptance: GET `/v1/repos/{repo_id}` returns full repo record. Repo detail page renders all sections.

- FR-1.3: Edit a repo's stacks from the detail page.
  - Acceptance: POST `/v1/repos/{repo_id}/update` with `{ stacks: [...] }` updates the repo and triggers workflow re-assembly. Console reflects updated stacks.

- FR-1.4: Delete a repo.
  - Acceptance: DELETE `/v1/repos/{repo_id}` removes the repo and all associated data (cascading foreign keys). Console reflects removal on next page load.

- FR-1.5: Detect repository languages from tracked files.
  - Acceptance: GET `/v1/repos/{repo_id}/languages` returns a `{ language: bytes }` map derived from `git ls-files`.

### FR-2: GitHub Issue Integration

The system fetches and caches open issues from GitHub for onboarded repositories.

- FR-2.1: Fetch issues for a repo using GitHub GraphQL API (with token) or `gh` CLI fallback.
  - Acceptance: GET `/v1/repos/{repo_id}/issues` returns `GitHubIssueRecord[]` with number, title, body, labels, state, and already_queued flag.

- FR-2.2: Cache fetched issues in SQLite with a 5-minute TTL.
  - Acceptance: Subsequent requests within 5 minutes return cached data without hitting GitHub. `?refresh=true` bypasses cache.

- FR-2.3: Display issues in the console with rendered markdown body previews, label badges, and an expand modal for full issue body.
  - Acceptance: IssueList component renders issue cards with truncated body (180 chars), expand button opens detail modal with full rendered markdown.

- FR-2.4: Filter issues by repository on the global issues page.
  - Acceptance: Repo dropdown filters visible issue cards; count badge updates to match.

### FR-3: Workflow System

The prompts repo (`agent-prompts`) is a read-only external dependency containing generic workflows, stack prompts, and PM prompts. Crabitat reads from it but never writes to it. All Crabitat-specific state (assembled workflows, sync records) lives in the local database.

- FR-3.1: Load workflow definitions from TOML files in the read-only prompts repo.
  - Acceptance: On startup and on POST `/v1/workflows/sync`, all `workflows/*.toml` files are parsed into `WorkflowManifest` structs and stored in an in-memory `WorkflowRegistry`. The prompts repo is not modified.

- FR-3.2: Each workflow manifest defines metadata (name, description, version, default include list) and an ordered list of steps, where each step has a prompt file, optional dependencies, optional condition, max retries, and optional include override.
  - Acceptance: A TOML file with `[workflow]` and `[[steps]]` sections loads correctly. Step dependencies expressed as `depends_on = ["step_id"]`.

- FR-3.3: Sync the in-memory registry to the SQLite database, upserting by workflow name and removing stale entries.
  - Acceptance: POST `/v1/workflows/sync` returns `SyncResult { synced, removed, commit_hash, errors }`. DB reflects current TOML state.

- FR-3.4: Display workflows in the console with name, version, source tag (TOML / ASSEMBLED / Manual), commit hash, description, and a step pipeline visualization.
  - Acceptance: WorkflowList component renders all workflows. Source tags use distinct colors (blue=TOML, green=ASSEMBLED, purple=Manual).

- FR-3.5: Browse and preview prompt files from the prompts repo in the console.
  - Acceptance: PromptBrowser component lists all `.md` files (excluding README/AGENTS/CLAUDE). Clicking a file shows rendered preview via GET `/v1/prompt-files/preview`.

- FR-3.6: Load prompt file content from disk at task expansion time.
  - Acceptance: `WorkflowRegistry::load_prompt_file()` reads files relative to the prompts repo root. Content is substituted into task prompts with `{{mission_prompt}}`, `{{context}}`, and `{{worktree_path}}` template variables.

### FR-4: Assembled Workflows (Stack Composition)

The system automatically composes base workflows with repo-specific stack prompts. The prompts repo provides the building blocks (generic workflows and stack/PM prompt files); Crabitat is the assembler that combines them per-repo and stores the results in its own database.

- FR-4.1: Discover available stacks by scanning `prompts/stacks/*.md` and `prompts/pm/*.md` in the read-only prompts repo.
  - Acceptance: GET `/v1/stacks` returns `[{ name, path }]` for all discovered stack prompts, sorted by name.

- FR-4.2: Build a stack map that resolves short names (e.g., "rust") to relative prompt paths (e.g., "prompts/stacks/rust.md").
  - Acceptance: `WorkflowRegistry.stack_map` is populated on load and used by `resolve_stacks()`.

- FR-4.3: For each unique stack combination across all repos, assemble a variant of every base workflow by merging the base workflow's existing includes with the resolved stack includes (deduplicated).
  - Acceptance: A repo with stacks `["rust", "mise"]` and a base workflow that already includes `["prompts/pm/github-issue.md"]` produces `develop-feature/mise+rust` with includes `["prompts/pm/github-issue.md", "prompts/stacks/mise.md", "prompts/stacks/rust.md"]`. If a stack duplicates an existing base include, it is not added twice.

- FR-4.4: Assembled workflows inherit steps from the base workflow unchanged and use the merged include list, which the existing task expansion logic appends to each step's rendered prompt.
  - Acceptance: `expand_workflow_into_tasks()` at `step.include.as_deref().unwrap_or(&manifest.workflow.include)` naturally uses the assembled includes. Prompt files are read from the prompts repo at task expansion time.

- FR-4.5: Re-assemble on startup, on workflow sync, and when any repo's stacks change.
  - Acceptance: Creating/updating a repo with stacks triggers `assemble_workflows()` followed by DB sync.

- FR-4.6: Assembled workflows are stored in the DB with `source = 'assembled'` and cleaned up when their stack combo no longer exists.
  - Acceptance: `sync_toml_workflows_to_db` handles both `toml` and `assembled` sources.

### FR-5: Issue Queue & Mission Pipeline

Operators queue GitHub issues to create missions that execute through workflow pipelines.

- FR-5.1: Queue an issue as a mission for a repo.
  - Acceptance: POST `/v1/repos/{repo_id}/queue` with `{ issue_number, workflow? }` creates a pending Mission linked to the issue. Duplicate queueing is rejected.

- FR-5.2: Compute the default workflow for a queued issue based on the repo's stacks.
  - Acceptance: If no workflow specified, use `develop-feature/{sorted_stacks}` if repo has stacks, else `develop-feature`.

- FR-5.3: Expand a mission's workflow into tasks with rendered prompts.
  - Acceptance: `expand_workflow_into_tasks()` creates one Task per workflow step. Each task's prompt includes the step's prompt template with `{{mission_prompt}}` and `{{worktree_path}}` substituted, plus all effective include files appended.

- FR-5.4: Respect task dependencies — tasks with `depends_on` start as Blocked and transition to Queued when dependencies complete.
  - Acceptance: `cascade_workflow()` unblocks dependent tasks. Condition evaluation (`step_id.field == 'value'`) gates task activation.

- FR-5.5: View the mission queue per repo and globally.
  - Acceptance: GET `/v1/repos/{repo_id}/queue` returns pending+running missions ordered by queue_position. IssueQueue component shows side-by-side issues and queue.

- FR-5.6: Remove a pending mission from the queue.
  - Acceptance: DELETE `/v1/repos/{repo_id}/queue/{mission_id}` removes the mission only if its status is Pending.

- FR-5.7: Display mission pipeline with step-level status visualization.
  - Acceptance: MissionPipeline and StepPipeline components render colored step indicators (queued, assigned, running, blocked, completed, failed).

- FR-5.8: Filter workflow dropdown in mission creation and issue launch modals based on the selected repo's stacks.
  - Acceptance: Dropdown shows assembled workflows matching the repo's stack combo, plus base workflows as fallback.

### FR-6: Task Scheduler

The scheduler automatically assigns queued tasks to idle agents.

- FR-6.1: Run a scheduler tick when triggered (registration, run completion, cascade, or manual).
  - Acceptance: `run_scheduler_tick_db()` iterates queued tasks (ordered by creation time) and idle crabs, producing `SchedulerAssignment` pairs.

- FR-6.2: Serialize execution within missions — at most one running task per mission to prevent worktree conflicts.
  - Acceptance: If any task in a mission has status `running`, other queued tasks in that mission are skipped during scheduling.

- FR-6.3: Parallelize across repos — tasks from different missions (in different repos) can run simultaneously on different agents.
  - Acceptance: Multiple agents can be busy at the same time with tasks from different repos.

- FR-6.4: Skip `merge-wait` tasks during scheduling — these are handled by the background merge-wait poller.
  - Acceptance: Tasks with `step_id = 'merge-wait'` are never assigned to crabs by the scheduler.

- FR-6.5: Update state on assignment — mark the task as `assigned`, set `assigned_crab_id`, and mark the crab as `busy`.
  - Acceptance: Both the `tasks` and `crabs` tables are updated atomically within the scheduler tick transaction.

### FR-7: Workflow Cascade Engine

When a task completes or fails, the cascade engine evaluates downstream dependencies and advances the mission pipeline.

- FR-7.1: On task completion, find all dependent tasks (via `task_deps` table) that are currently Blocked.
  - Acceptance: `cascade_workflow()` queries `task_deps WHERE depends_on_task_id = ?` to find downstream tasks.

- FR-7.2: Check if all dependencies of a blocked task are terminal (Completed or Skipped). Only unblock if all are resolved.
  - Acceptance: A task with two dependencies requires both to complete before it transitions from Blocked to Queued.

- FR-7.3: Evaluate step conditions against a context map built from completed run results in the mission.
  - Acceptance: `evaluate_condition("review.result == 'PASS'", context)` returns true when the review step's result is "PASS". Conditions use the format `step_id.field == 'value'`.

- FR-7.4: Skip tasks whose conditions evaluate to false, and recursively cascade from skipped tasks.
  - Acceptance: If a "fix" step has condition `review.result == 'FAIL'` and the review passes, the fix step is skipped and its dependents are evaluated.

- FR-7.5: Build accumulated context from the dependency chain and attach to unblocked tasks.
  - Acceptance: When a task is unblocked, its `context` column is populated with results from ancestor tasks, making upstream outputs available to downstream prompts.

- FR-7.6: Cascade failure — when a task fails, propagate failure to all transitive dependents.
  - Acceptance: `cascade_failure()` marks all downstream tasks as Failed and updates the mission status.

- FR-7.7: Handle review-fix retry loops — when a "fix" step completes, re-queue the "review" step.
  - Acceptance: `requeue_review_after_fix()` resets the review task to Queued status, enabling iterative quality improvement.

- FR-7.8: Capture PR numbers — when a "pr" step completes, extract the PR number from the run result and store it on the mission.
  - Acceptance: `missions.github_pr_number` is populated from `context_map["pr.result"]` after the PR step completes.

- FR-7.9: Update mission status after cascade — compute overall mission status from task states.
  - Acceptance: `update_mission_status()` transitions the mission to Running/Completed/Failed based on the aggregate state of its tasks.

### FR-8: Merge-Wait Poller

A background process polls GitHub for PR merge status to advance missions past the merge-wait step.

- FR-8.1: Run a background polling loop every 60 seconds.
  - Acceptance: `spawn_merge_wait_poller()` ticks every 60 seconds and calls `poll_merge_wait_tasks()`.

- FR-8.2: Find all queued merge-wait tasks, look up their associated PR numbers and repo identifiers.
  - Acceptance: Query joins `tasks`, `missions`, and `repos` to find `step_id = 'merge-wait' AND status = 'queued'` with the PR number and `owner/name`.

- FR-8.3: Check each PR's status via GitHub API (`get_pr_status()`).
  - Acceptance: For each PR, the poller calls the GitHub API and gets back `state` (OPEN/MERGED/CLOSED) and `merged_at`.

- FR-8.4: On PR merged — create a synthetic completed run, mark the merge-wait task as completed, cascade the workflow, and run the scheduler.
  - Acceptance: A system run with `crab_id = 'system'` is inserted, the task transitions to `completed`, and downstream tasks are evaluated.

- FR-8.5: On PR closed without merge — mark the merge-wait task as failed and cascade failure.
  - Acceptance: The task transitions to `failed`, triggering `cascade_workflow()` to propagate failure to dependents.

### FR-9: Agent Management

Operators view and manage the fleet of coding agents.

- FR-9.1: Register agents with a repo assignment.
  - Acceptance: POST `/v1/crabs/register` with `{ crab_id, repo_id, name }` creates an agent record in idle state and immediately runs the scheduler.

- FR-9.2: Display agent status grid showing each agent's state (idle/busy/offline), assigned repo, current task, and last update time.
  - Acceptance: CrabGrid component renders agent cards with state-colored indicators.

- FR-9.3: List all agents.
  - Acceptance: GET `/v1/crabs` returns all registered agents with current state.

### FR-10: Run Lifecycle

Runs track individual execution attempts of tasks by agents.

- FR-10.1: Start a run for an assigned task.
  - Acceptance: POST `/v1/runs/start` with `{ run_id, mission_id, task_id, crab_id, burrow_path, burrow_mode }` creates a run record and transitions the task to Running.

- FR-10.2: Update a run with progress information.
  - Acceptance: POST `/v1/runs/update` with `{ run_id, status, note, metrics }` updates the run's state and metrics.

- FR-10.3: Complete a run with final status, summary, timing, and token usage.
  - Acceptance: POST `/v1/runs/complete` with `{ run_id, status, summary, timing, token_usage }` marks the run as completed/failed, transitions the crab to idle, cascades the workflow, and runs the scheduler.

- FR-10.4: Track run metrics — prompt tokens, completion tokens, total tokens, timing breakdowns.
  - Acceptance: `RunMetrics` struct stores `prompt_tokens`, `completion_tokens`, `total_tokens`, `first_token_ms`, `llm_duration_ms`, `execution_duration_ms`, `end_to_end_ms`.

### FR-11: Agent CLI (crabitat-crab)

The crab CLI provides the agent runtime for interacting with the control-plane.

- FR-11.1: Register with the control-plane via `register` subcommand.
  - Acceptance: `crabitat-crab register --repo-id <ID> --name <NAME>` calls POST `/v1/crabs/register` and prints JSON with the assigned `crab_id`.

- FR-11.2: Poll for assigned tasks via `poll` subcommand.
  - Acceptance: `crabitat-crab poll --crab-id <ID>` calls GET `/v1/tasks`, filters for tasks assigned to this crab with status `queued` or `assigned`, and prints the first match as JSON. Prints nothing if no task is pending.

- FR-11.3: Start a run via `start-run` subcommand.
  - Acceptance: `crabitat-crab start-run --mission-id <M> --task-id <T> --crab-id <C>` calls POST `/v1/runs/start` with a generated `run_id` and prints the response.

- FR-11.4: Complete a run via `complete-run` subcommand.
  - Acceptance: `crabitat-crab complete-run --run-id <R> --status <S>` calls POST `/v1/runs/complete` with optional `--summary`, `--result`, `--duration-ms`, `--prompt-tokens`, `--completion-tokens`, `--total-tokens`. Result and summary are combined into a JSON structure for workflow condition evaluation.

- FR-11.5: Print onboarding instructions via `guide` subcommand.
  - Acceptance: `crabitat-crab guide` outputs a multi-step guide that a Claude Code agent can follow to register, poll, execute, and complete tasks autonomously.

- FR-11.6: Query system state via `status`, `missions`, and `tasks` subcommands.
  - Acceptance: Each subcommand fetches the corresponding API endpoint and pretty-prints the JSON response.

### FR-12: Console

The console provides the operator UI as a server-rendered web application.

- FR-12.1: Full status snapshot endpoint for page load.
  - Acceptance: GET `/v1/status` returns StatusSnapshot with summary stats, all repos, crabs, missions, tasks, runs, and repo issue counts.

- FR-12.2: Console sidebar shows counts for repos, issues, missions, agents, and workflows.
  - Acceptance: Sidebar component displays counts; active page is highlighted.

---

## Data Model

### Entity Hierarchy

```
Repo (1) ──→ (N) Crab (agent)
  │              │
  └──→ (N) Mission ──→ (N) Task ──→ (N) Run
              │              │
              │              └── task_deps (M:N)
              │
              └── github_issue_number, github_pr_number
```

### Tables

| Table | Primary Key | Purpose |
|-------|-------------|---------|
| `repos` | `repo_id` | Onboarded GitHub repositories with local path, language, stacks |
| `crabs` | `crab_id` | Registered agents with state (idle/busy/offline) and current task |
| `missions` | `mission_id` | Issue-backed work units with workflow, queue position, PR tracking |
| `tasks` | `task_id` | Individual workflow steps with status, prompt, step_id, assigned crab |
| `task_deps` | `(task_id, depends_on_task_id)` | Directed dependency edges between tasks |
| `runs` | `run_id` | Execution attempts with metrics, timing, token usage, summary |
| `workflows` | `workflow_id` | Synced workflow definitions (TOML, assembled, or manual) |
| `workflow_steps` | `(workflow_id, position)` | Ordered steps within a workflow |
| `settings` | `key` | Key-value configuration store |
| `github_issues_cache` | `(repo_id, number)` | Cached GitHub issues with 5-minute TTL |

### State Machines

**Mission:** `Pending → Running → Completed | Failed`

**Task:** `Queued → Assigned → Running → Completed | Failed`
         `Blocked → Queued` (when dependencies resolve)
         `Blocked → Skipped` (when condition evaluates false)

**Run:** `Queued → Running → Completed | Failed`

**Crab:** `idle → busy → idle` (on task assignment / run completion)

---

## API Reference

33 endpoints across 8 resource groups.

### System
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/healthz` | `healthz` | Health check |
| GET | `/v1/status` | `get_status` | System status summary |
| GET | `/v1/settings` | `get_settings` | Read configuration |
| POST | `/v1/settings` | `patch_settings` | Update configuration |

### Repos
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/repos` | `create_repo` | Onboard a repository |
| GET | `/v1/repos` | `list_repos` | List all repositories |
| GET | `/v1/repos/{repo_id}` | `get_repo` | Get single repo |
| DELETE | `/v1/repos/{repo_id}` | `delete_repo` | Remove a repository |
| POST | `/v1/repos/{repo_id}/update` | `update_repo` | Update repo (stacks, etc.) |
| GET | `/v1/repos/{repo_id}/languages` | `get_repo_languages` | Detect repo languages |
| GET | `/v1/repos/{repo_id}/issues` | `list_repo_issues` | Fetch GitHub issues for repo |

### Queue
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/repos/{repo_id}/queue` | `list_queue` | List queued missions for repo |
| POST | `/v1/repos/{repo_id}/queue` | `queue_issue` | Queue an issue as a mission |
| DELETE | `/v1/repos/{repo_id}/queue/{mission_id}` | `remove_from_queue` | Remove mission from queue |

### Agents
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/crabs` | `list_crabs` | List registered agents |
| POST | `/v1/crabs/register` | `register_crab` | Register a new agent |

### Missions
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/missions` | `create_mission` | Create a mission |
| GET | `/v1/missions` | `list_missions` | List all missions |
| GET | `/v1/missions/{mission_id}` | `get_mission` | Get mission detail |

### Tasks
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/tasks` | `create_task` | Create a task |
| GET | `/v1/tasks` | `list_tasks` | List tasks (filterable) |

### Runs
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/runs/start` | `start_run` | Agent claims a task run |
| POST | `/v1/runs/update` | `update_run` | Agent posts run progress |
| POST | `/v1/runs/complete` | `complete_run` | Agent completes a run |

### Workflows
| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/workflows` | `list_db_workflows` | List all workflows |
| POST | `/v1/workflows` | `create_workflow` | Create a workflow |
| POST | `/v1/workflows/sync` | `sync_workflows` | Sync TOML workflows to DB |
| GET | `/v1/workflows/{workflow_id}` | `get_workflow` | Get workflow detail |
| DELETE | `/v1/workflows/{workflow_id}` | `delete_workflow` | Delete a workflow |
| POST | `/v1/workflows/{workflow_id}/update` | `update_workflow` | Update a workflow |
| GET | `/v1/stacks` | `list_stacks` | List discovered stack prompts |
| GET | `/v1/prompt-files` | `list_prompt_files` | List prompt files in repo |
| GET | `/v1/prompt-files/preview` | `preview_prompt_file` | Preview a prompt file |
| GET | `/v1/skills` | `list_skills` | List available skills |

---

## Non-Functional Requirements

- NFR-1: Single-machine deployment with SQLite and local filesystem.
  - Acceptance: The system runs with a single `crabitat-control-plane serve` command and a local DB file.

- NFR-2: Startup time under 2 seconds including DB migration, workflow load, assembly, and sync.
  - Acceptance: Measured from process start to "listening on" log line.

- NFR-3: Console pages load within 500ms (SSR with Astro + Node adapter).
  - Acceptance: All pages render server-side with data fetched from the control-plane API.

- NFR-4: GitHub issue cache reduces API calls by 95%+ under normal usage (5-minute TTL).
  - Acceptance: Repeated page loads within 5 minutes produce zero GitHub API calls.

- NFR-5: Workflow assembly is idempotent — running it multiple times with the same inputs produces identical results.
  - Acceptance: `assemble_workflows()` removes all previous assembled entries before rebuilding.

- NFR-6: Database uses WAL mode and foreign keys for data integrity and concurrent read performance.
  - Acceptance: `PRAGMA journal_mode = WAL` and `PRAGMA foreign_keys = ON` are set at connection time.

- NFR-7: Agent summaries are capped at 4 KiB to prevent oversized payloads.
  - Acceptance: Both the crab CLI and control-plane enforce summary length limits.

---

## Technical Constraints

- **Language:** Rust (control-plane, crab CLI, core library, protocol), TypeScript (console)
- **Framework:** Axum (HTTP server), Astro (SSR console with Node adapter)
- **Database:** SQLite via rusqlite (WAL mode, foreign keys enabled)
- **Build tool:** Cargo (Rust workspace), Bun (console)
- **Task runner:** mise (fmt, clippy, test, verify, build, console-*)
- **Package manager:** Bun only — no npm, npx, yarn, or pnpm
- **GitHub API:** GraphQL with `GITHUB_TOKEN`, or `gh` CLI fallback
- **IDs:** UUID-based (`MissionId`, `TaskId`, `RunId` as Uuid newtypes) and `kiters::eid::ExternalId` for some entity IDs
- **Agent runtime:** Claude Code CLI (`claude -p "<prompt>" --output-format json`)
- **Deployment:** Single machine, single process, local git worktrees

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `crabitat-core` | Shared types: IDs, enums (TaskStatus, MissionStatus, RunStatus, BurrowMode), workflow manifest structs, condition evaluator, RunMetrics |
| `crabitat-protocol` | Wire protocol types (reserved for future use) |
| `crabitat-control-plane` | Axum server: API handlers, SQLite schema, scheduler, cascade engine, merge-wait poller, workflow registry, GitHub client |
| `crabitat-crab` | Agent CLI: register, poll, start-run, complete-run, guide, status/missions/tasks queries |
| `crabitat-console` | Astro SSR app: pages, components, API proxy routes, client-side scripts |

---

## Non-Goals

- Distributed deployment across multiple machines
- Support for issue trackers other than GitHub (Jira, Linear, etc.)
- User authentication or multi-tenancy in the console
- CI/CD integration or automated deployment
- Billing or usage metering
- Mobile-responsive console layout
- Agent runtime support for LLMs other than Claude Code (v1)

---

## Success Metrics

1. **Onboarding speed** — a new repo is onboarded (with stacks, issues visible) in under 60 seconds.
2. **Issue-to-running** — time from "queue issue" click to first agent task executing is under 5 seconds (given an idle agent).
3. **Workflow sync** — sync completes with 0 errors for a valid prompts repo.
4. **Assembly correctness** — every repo's assembled workflows contain exactly the merged base + stack includes, validated by expanding a task and inspecting the rendered prompt.
5. **Cascade latency** — time from run completion to dependent task being queued is under 100ms.
7. **Agent autonomy** — a crab can register, poll, execute, and complete 10 consecutive tasks without manual intervention.
8. **Merge-wait reliability** — PR merge status is detected within 60 seconds and the workflow advances correctly.
