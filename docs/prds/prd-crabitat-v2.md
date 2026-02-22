# PRD: Crabitat v2 — UI-First Autonomous Agent Orchestration

**Status:** Draft
**Date:** 2026-02-22

---

## 1. Objective

A minimal UI that lets a human operator:

1. Onboard GitHub repos and view their open issues
2. Convert issues into missions
3. Hit "Execute" and watch autonomous TDD workflow run — agents are matched automatically
4. See agent executions, token usage, wall clock time, and inter-agent messages

## 2. Architecture

**One control plane, multiple repos.** A single crabitat control plane manages multiple onboarded repos. All repos share a single work queue. Agents are a shared pool — they pick up tasks from any repo based on capability matching.

## 3. Entities

### Repo

An onboarded GitHub repository. Config:

| Field | Example |
|-------|---------|
| `repo_id` | auto-generated |
| `owner` | `l1x` |
| `name` | `crabitat` |
| `full_name` | `l1x/crabitat` |
| `default_branch` | `main` |
| `domain` | `backend` |
| `local_path` | `/Users/l1x/code/home/projectz/crabitat` |

The `domain` tag classifies what kind of codebase this is. Used for agent matching.

**Domains:** `web-dev`, `backend`, `infra`, `docs`, `any`

### Agent

A generic Claude Code process that:

- Registers with crabitat (`crabitat-agent register --name Y --domains web-dev,backend --roles code,plan`)
- Polls `GET /v1/agents/{id}/next` every 10 seconds
- Gets a task with a **system prompt** injected for that specific assignment
- Executes via: `claude -p "<task_prompt>" --system-prompt "<role_prompt>" --dangerously-skip-permissions --output-format json`
- Reports results with token usage and timing

**Agent capabilities — two dimensions:**

| Dimension | Values | Meaning |
|-----------|--------|---------|
| **Domains** | `any`, `web-dev`, `backend`, `infra`, `docs` | What codebases the agent can work on |
| **Roles** | `any`, `plan`, `code`, `review` | What pipeline steps the agent can perform |

Examples:

```
Agent "atlas"     → domains: [any]      roles: [any]        # jack of all trades
Agent "websmith"  → domains: [web-dev]  roles: [code, plan] # web specialist
Agent "critic"    → domains: [any]      roles: [review]     # dedicated reviewer
Agent "docbot"    → domains: [docs]     roles: [any]        # docs only
Agent "ops"       → domains: [infra]    roles: [any]        # infra only
```

**Agent lifecycle:** `idle` → picks up task → `busy` → completes → `idle`

Persistent agents (doc-search, infra services) are always-on and respond to messages.

### Mission

A unit of work derived from a GitHub issue. Belongs to a repo. Has a fixed 4-phase pipeline:

1. **Plan** — agent reads issue, explores codebase, posts plan as issue comment
2. **Test** — agent writes failing tests based on plan (TDD red)
3. **Implement** — agent makes tests pass (TDD green), pushes to branch
4. **Review** — agent reviews diff, outputs PASS/FAIL

If review fails → loop back to implement (max 3 retries). If review passes → create PR.

### Message

Agent-to-agent communication. Used by persistent agents (doc-search responds to queries) and for collaboration. All messages visible on UI.

## 4. Scheduler

The scheduler matches tasks to agents using two-dimensional capability matching:

```
agent picks up task IF:
  (task.domain ∈ agent.domains  OR  agent.domains contains "any")
  AND
  (task.role ∈ agent.roles  OR  agent.roles contains "any")
```

When multiple agents match, prefer:
1. Idle agents over busy agents
2. Specific domain match over `any`
3. Most recent heartbeat (agent freshness)

Tasks sit in a shared queue ordered by creation time. Agents poll every 10s and get the first matching task.

## 5. TDD Workflow Pipeline

```
Issue → [Plan] → [Write Test] → [Implement+Verify] → [Review] → PR
                                        ↑                |
                                        └── FAIL ────────┘
```

Each phase is a task with a `role` tag (`plan`, `code`, `code`, `review`). Tasks run sequentially in a shared worktree. The control plane cascades: when a task completes, the next one unblocks.

**Prompts** live in `agent-prompts/do/` and reference skills from `agent-prompts/skills/`. Template variables: `{{mission_prompt}}`, `{{context}}`, `{{worktree_path}}`, `{{github_issue_number}}`, `{{github_repo}}`.

## 6. Minimum UI (4 Views)

### View 1: Issues

- Repo selector dropdown (onboarded repos)
- Lists open GitHub issues from the selected repo
- Each issue has a "Create Mission" button
- Creating a mission: confirms workflow (default: TDD), agents are matched automatically by scheduler
- "Execute" button starts the pipeline

### View 2: Missions

- Pipeline view per mission: Plan → Test → Implement → Review
- Each phase shows: status badge, assigned agent, token count, wall clock time
- Expandable: shows run details, agent output summary
- Link to GitHub issue and PR
- Filterable by repo

### View 3: Agents

- Lists registered agents (both task and persistent)
- Shows: name, domains, roles, state (idle/busy/offline), current mission, last heartbeat
- For persistent agents: shows message count
- Agent capabilities visible at a glance via domain + role tags

### View 4: Messages

- Chronological log of agent-to-agent messages
- Filterable by mission, agent, or message type
- Shows: from, to, body, timestamp, mission context

## 7. Agent Polling Protocol

```
Agent starts:
  POST /v1/agents/register
  {name, domains: ["web-dev","backend"], roles: ["code","plan"], type: "task"|"persistent"}
  → returns {agent_id}

Every 10 seconds:
  GET /v1/agents/{id}/next
  → {"status": "idle"}                     -- nothing to do
  → {"status": "task", "task": {...},       -- execute this
     "mission": {...}, "system_prompt": "..."}
  → {"status": "message", "message": {...}} -- for persistent agents

After execution:
  POST /v1/runs/complete {run_id, status, summary, token_usage, timing}
```

## 8. Messaging Protocol

```
POST /v1/messages
{
  "from": "agent-id-1",
  "to": "agent-id-2",
  "mission_id": "optional",
  "body": "What's the auth module structure?"
}

Response arrives on next poll:
GET /v1/agents/{id}/next
→ {"status": "message", "message": {"from": "...", "body": "..."}}
```

Messages are stored and broadcast to the console UI via WebSocket.

## 9. Schema

### `repos` table

```sql
CREATE TABLE repos (
  repo_id TEXT PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL UNIQUE,   -- "owner/name"
  default_branch TEXT NOT NULL DEFAULT 'main',
  domain TEXT NOT NULL DEFAULT 'any',
  local_path TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
```

### `agents` table

```sql
CREATE TABLE agents (
  agent_id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  agent_type TEXT NOT NULL DEFAULT 'task',  -- 'task' or 'persistent'
  domains TEXT NOT NULL DEFAULT 'any',      -- comma-separated: "web-dev,backend" or "any"
  roles TEXT NOT NULL DEFAULT 'any',        -- comma-separated: "code,plan" or "any"
  state TEXT NOT NULL DEFAULT 'idle',       -- 'idle', 'busy', 'offline'
  current_mission_id TEXT,
  last_heartbeat_ms INTEGER,
  created_at_ms INTEGER NOT NULL
);
```

### `missions` table

```sql
CREATE TABLE missions (
  mission_id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repos(repo_id),
  title TEXT NOT NULL,
  github_issue_number INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',  -- 'pending','running','completed','failed'
  branch TEXT,
  pr_number INTEGER,
  created_at_ms INTEGER NOT NULL
);
```

### `tasks` table

```sql
CREATE TABLE tasks (
  task_id TEXT PRIMARY KEY,
  mission_id TEXT NOT NULL REFERENCES missions(mission_id),
  step TEXT NOT NULL,           -- 'plan','test','implement','review'
  role TEXT NOT NULL,           -- 'plan','code','code','review' (for scheduler matching)
  status TEXT NOT NULL DEFAULT 'pending',
  assigned_agent_id TEXT,
  system_prompt TEXT,
  summary TEXT,
  tokens INTEGER NOT NULL DEFAULT 0,
  wall_clock_ms INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
```

### `messages` table

```sql
CREATE TABLE messages (
  message_id TEXT PRIMARY KEY,
  from_agent_id TEXT NOT NULL,
  to_agent_id TEXT NOT NULL,
  mission_id TEXT,
  body TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
```

## 10. Non-Goals (v2)

- Workflow editor in UI (TOML files are source of truth)
- Parallel steps within a mission
- Agent cost budgets (future)
- Custom domain/role values via UI (config only for now)
