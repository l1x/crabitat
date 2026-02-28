# Crabitat Console — Product Requirements Document

**Version:** 1.0
**Date:** 2026-02-28

---

## Overview

Crabitat is a local-first orchestration platform for autonomous coding agents. It manages a fleet of AI agents ("crabs") that execute multi-step development workflows against GitHub repositories on a single machine. The system takes GitHub issues as input, expands them through configurable workflow pipelines, and dispatches tasks to agents that work in isolated git worktrees.

The **Crabitat Console** is the web-based control surface for operating the platform.

---

## Problem Statement

Running autonomous coding agents at scale on a codebase requires coordination that doesn't exist out of the box. Individual agents can execute prompts, but there is no system for:

- **Sequencing work** — a feature implementation requires planning, coding, QA, and review steps in order, with dependencies and retry logic.
- **Parallelizing across repos** — an organization manages dozens of repositories, each needing its own agent pool and issue queue.
- **Composing prompts** — each repo has a unique tech stack (Rust, Astro, mise, etc.) and the same base workflow needs different stack-specific context appended per repo.
- **Visibility** — knowing which agents are idle, which missions are running, what step they're on, and whether a PR has merged.

Without this, operators manually invoke agents per-issue, copy-paste context, and lose track of what's running where.

---

## Goals

1. **Repos as units of management** — onboard GitHub repositories with local paths and declared tech stacks; fetch and cache their issues automatically.
2. **Declarative workflows** — define multi-step agent workflows in TOML, version them in a prompts repo, and sync them into the system.
3. **Assembled workflows** — automatically combine base workflows with repo-specific stack prompts so each repo gets the right context without manual wiring.
4. **Issue-to-mission pipeline** — queue GitHub issues as missions, expand them into dependent tasks via workflows, and dispatch tasks to available agents.
5. **Operational visibility** — a real-time console showing repos, issues, missions, task pipelines, agent status, and workflow configuration.

---

## Job Stories

1. As an operator, I can onboard a repository with its local path and tech stacks, so that the system knows where to run agents and what prompts to include.
2. As an operator, I can view open GitHub issues for any onboarded repo and queue them as missions with one click, so that work enters the pipeline without manual prompt writing.
3. As an operator, I can sync workflow definitions from a prompts repo, so that the team's workflow changes take effect immediately.
4. As an operator, I can see which agents are idle, busy, or offline and what task each is working on, so that I can diagnose bottlenecks.
5. As an operator, I can see the full pipeline of a mission — which steps are queued, running, blocked, completed, or failed — so that I understand progress at a glance.
6. As an operator, I can select which assembled workflow to use when queuing an issue, so that I can override the default for special cases.

---

## Assumptions

1. The platform runs on a single machine with local filesystem access to all repo clones.
2. SQLite is sufficient for the control-plane database at the expected scale (tens of repos, hundreds of missions).
3. GitHub is the source of truth for issues; no other issue trackers are supported in v1.
4. Agents (crabs) are long-running processes that connect to the control-plane over WebSocket on the same machine.
5. The prompts repo is a git repository (or submodule) on the local filesystem.
6. The `gh` CLI or a `GITHUB_TOKEN` environment variable provides GitHub API access.

---

## Functional Requirements

### FR-1: Repository Management

Operators can onboard, update, and remove GitHub repositories.

- FR-1.1: Create a repo by selecting from GitHub (via `gh` CLI), choosing a local directory path, and optionally selecting tech stacks.
  - Acceptance: POST `/v1/repos` creates a repo record with owner, name, default_branch, language, local_path, and stacks. Console reflects the new repo immediately via WebSocket event.

- FR-1.2: View repo details including branch, language, local path, language breakdown bar, stacks, open issues, workflows, and mission pipeline.
  - Acceptance: GET `/v1/repos/{repo_id}` returns full repo record. Repo detail page renders all sections.

- FR-1.3: Edit a repo's stacks from the detail page.
  - Acceptance: POST `/v1/repos/{repo_id}/update` with `{ stacks: [...] }` updates the repo and triggers workflow re-assembly. Console reflects updated stacks.

- FR-1.4: Delete a repo.
  - Acceptance: DELETE `/v1/repos/{repo_id}` removes the repo and all associated data (cascading foreign keys). Console reflects removal via WebSocket event.

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

Operators define, sync, and manage multi-step agent workflows.

- FR-3.1: Load workflow definitions from TOML files in the prompts repo.
  - Acceptance: On startup and on POST `/v1/workflows/sync`, all `workflows/*.toml` files are parsed into `WorkflowManifest` structs and stored in an in-memory registry.

- FR-3.2: Each workflow manifest defines metadata (name, description, version, default include list) and an ordered list of steps, where each step has a prompt file, optional dependencies, optional condition, max retries, and optional include override.
  - Acceptance: A TOML file with `[workflow]` and `[[steps]]` sections loads correctly. Step dependencies expressed as `depends_on = ["step_id"]`.

- FR-3.3: Sync the in-memory registry to the SQLite database, upserting by workflow name and removing stale entries.
  - Acceptance: POST `/v1/workflows/sync` returns `SyncResult { synced, removed, commit_hash, errors }`. DB reflects current TOML state.

- FR-3.4: Display workflows in the console with name, version, source tag (TOML / ASSEMBLED / Manual), commit hash, description, and a step pipeline visualization.
  - Acceptance: WorkflowList component renders all workflows. Source tags use distinct colors (blue=TOML, green=ASSEMBLED, purple=Manual).

- FR-3.5: Browse and preview prompt files from the prompts repo in the console.
  - Acceptance: PromptBrowser component lists all `.md` files (excluding README/AGENTS/CLAUDE). Clicking a file shows rendered preview.

### FR-4: Assembled Workflows (Stack Composition)

The system automatically composes base workflows with repo-specific stack prompts.

- FR-4.1: Discover available stacks by scanning `prompts/stacks/*.md` and `prompts/pm/*.md` in the prompts repo.
  - Acceptance: GET `/v1/stacks` returns `[{ name, path }]` for all discovered stack prompts, sorted by name.

- FR-4.2: Build a stack map that resolves short names (e.g., "rust") to relative prompt paths (e.g., "prompts/stacks/rust.md").
  - Acceptance: `WorkflowRegistry.stack_map` is populated on load and used by `resolve_stacks()`.

- FR-4.3: For each unique stack combination across all repos, assemble a variant of every base workflow with the resolved stack includes.
  - Acceptance: A repo with stacks `["rust", "mise", "github-issue"]` produces assembled workflows like `develop-feature/github-issue+mise+rust` (sorted alphabetically, joined with `+`).

- FR-4.4: Assembled workflows use the resolved stack file paths as their `include` list, which the existing task expansion logic appends to each step's rendered prompt.
  - Acceptance: `expand_workflow_into_tasks()` at `step.include.as_deref().unwrap_or(&manifest.workflow.include)` naturally uses the assembled includes.

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

### FR-6: Agent Management

Operators view and manage the fleet of coding agents.

- FR-6.1: Register agents with a repo assignment.
  - Acceptance: POST `/v1/crabs/register` with `{ crab_id, repo_id, name }` creates an agent record in idle state and immediately runs the scheduler.

- FR-6.2: Display agent status grid showing each agent's state (idle/busy/offline), assigned repo, current task, and last update time.
  - Acceptance: CrabGrid component renders agent cards with state-colored indicators.

- FR-6.3: Automatically assign queued tasks to idle agents via the scheduler.
  - Acceptance: `run_scheduler_tick_db()` matches queued tasks to idle crabs, updates both records, and emits WebSocket events. Serializes within missions (one running task per mission for worktree safety). Parallelizes across repos.

- FR-6.4: List all agents.
  - Acceptance: GET `/v1/crabs` returns all registered agents with current state.

### FR-7: Real-Time Console

The console provides live updates without page refreshes.

- FR-7.1: WebSocket connection from console to control-plane pushes events for all entity changes.
  - Acceptance: `/v1/ws/console` streams ConsoleEvent messages: snapshot, crab_updated, mission_created/updated, task_created/updated, run_created/updated/completed, repo_created/updated/deleted.

- FR-7.2: Full status snapshot endpoint for initial page load.
  - Acceptance: GET `/v1/status` returns StatusSnapshot with summary stats, all repos, crabs, missions, tasks, runs, and repo issue counts.

- FR-7.3: Console sidebar shows counts for repos, issues, missions, agents, and workflows.
  - Acceptance: Sidebar component displays counts; active page is highlighted.

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

---

## Technical Constraints

- **Language:** Rust (control-plane, crab runtime, core library), TypeScript (console)
- **Framework:** Axum (HTTP server), Astro (SSR console with Node adapter)
- **Database:** SQLite via rusqlite (WAL mode, foreign keys enabled)
- **Build tool:** Cargo (Rust workspace), Bun (console)
- **Task runner:** mise (fmt, clippy, test, verify, build, console-*)
- **Package manager:** Bun only — no npm, npx, yarn, or pnpm
- **GitHub API:** GraphQL with `GITHUB_TOKEN`, or `gh` CLI fallback
- **IDs:** `kiters::eid::ExternalId` format (e.g., `repo-abc123`, `wf-def456`)
- **WebSocket:** axum extract for crab and console connections
- **Deployment:** Single machine, single process, local git worktrees

---

## Non-Goals

- Distributed deployment across multiple machines
- Support for issue trackers other than GitHub (Jira, Linear, etc.)
- User authentication or multi-tenancy in the console
- Agent runtime implementation details (covered by crabitat-crab crate separately)
- CI/CD integration or automated deployment
- Billing or usage metering
- Mobile-responsive console layout

---

## Success Metrics

1. **Onboarding speed** — a new repo is onboarded (with stacks, issues visible) in under 60 seconds.
2. **Issue-to-running** — time from "queue issue" click to first agent task executing is under 5 seconds (given an idle agent).
3. **Workflow sync** — sync completes with 0 errors for a valid prompts repo.
4. **Assembly correctness** — every repo's assembled workflows contain exactly the resolved stack includes, validated by expanding a task and inspecting the rendered prompt.
5. **Console freshness** — WebSocket events propagate entity changes to the console within 100ms.
