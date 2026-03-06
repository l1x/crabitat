# Crabitat — Product Requirements Document

**Version:** 0.1
**Date:** 2026-03-05

---

## Overview

Crabitat is a local-first orchestration platform for autonomous coding agents. It manages workflows that turn GitHub issues into executable missions. The system consists of two components:

1. **Control-Plane** — Rust backend serving the REST API and managing the SQLite database.
2. **Console** — Web UI for operators to manage repos, issues, workflows, and missions.

---

## Core Concepts

1. **Repo** is onboarded — the user connects a GitHub repository to Crabitat.
2. **Issues** are loaded from that repo.
3. **Workflows** are user-defined, repo-scoped templates for any purpose (feature implementation, project management, bug triage, etc.), each with one or more **flavors** (e.g. `implement-feature/rust`, `implement-feature/astro`). Flavors are persisted in the database. A repo can have multiple flavors.
4. **Mission** = issue + workflow flavor → produces **tasks** → tasks produce **runs**.

---

## FR-1: Repository Management

Operators onboard GitHub repositories so the system knows what to work on.

- **FR-1.1**: Create a repo by providing owner, name, and local filesystem path.
- **FR-1.2**: List all onboarded repos.
- **FR-1.3**: View a single repo's details (owner, name, path, created_at).
- **FR-1.4**: Delete a repo and all its associated data (issues cache, workflows, missions — cascading).

---

## FR-2: Issue Loading

The system fetches and caches open issues from GitHub for onboarded repos.

- **FR-2.1**: Fetch open issues for a repo using the GitHub API (`gh` CLI or `GITHUB_TOKEN`).
- **FR-2.2**: Cache fetched issues in SQLite. No TTL — cache is populated on first access and updated only when the user explicitly triggers a refresh.
- **FR-2.3**: List issues for a repo, including number, title, body, labels, and state.
- **FR-2.4**: Display issues in the console with title, labels, and truncated body preview.

---

## FR-3: Workflows and Flavors

Workflows are user-defined templates scoped to a repo. Each workflow has one or more flavors.

- **FR-3.1**: Create a workflow for a repo with a name, description, and ordered list of steps. Each step has a name and a prompt template.
- **FR-3.2**: Create one or more flavors for a workflow. A flavor has a name (e.g. `rust`, `astro`) and optional additional context that gets injected into step prompts.
- **FR-3.3**: List workflows for a repo, showing name, description, and flavor count.
- **FR-3.4**: View a workflow's details including its steps and flavors.
- **FR-3.5**: Update a workflow (name, description, steps).
- **FR-3.6**: Delete a workflow and its flavors.
- **FR-3.7**: Display workflows and their flavors in the console.

---

## FR-4: Missions, Tasks, and Runs

A mission is created by combining an issue with a workflow flavor. It expands into tasks which produce runs.

- **FR-4.1**: Create a mission by selecting an issue and a workflow flavor. The mission expands the workflow's steps into individual tasks.
- **FR-4.2**: Each task gets its prompt from the workflow step template, with the flavor's context and the issue body injected.
- **FR-4.3**: Tasks execute in order (sequential by default). A task produces a run when it executes.
- **FR-4.4**: Track run status: queued → running → completed | failed.
- **FR-4.5**: Track mission status derived from its tasks: pending → running → completed | failed.
- **FR-4.6**: List missions globally and per repo.
- **FR-4.7**: View mission details with its task pipeline and run history.
- **FR-4.8**: Display mission pipeline in the console with step-level status indicators.

---

## Data Model

```
Repo (1) ──→ (N) Workflow ──→ (N) Flavor
  │                              │
  └──→ (N) Issue (cached)       │
                                │
Mission = Issue + Flavor ──→ (N) Task ──→ (N) Run
```

### Tables

| Table | Primary Key | Purpose |
|-------|-------------|---------|
| `repos` | `repo_id` | Onboarded GitHub repositories |
| `github_issues_cache` | `(repo_id, number)` | Cached GitHub issues, refreshed on demand |
| `workflows` | `workflow_id` | User-defined workflow templates, scoped to a repo |
| `workflow_steps` | `step_id` | Ordered steps within a workflow |
| `workflow_flavors` | `flavor_id` | Flavors of a workflow (e.g. rust, astro) |
| `missions` | `mission_id` | Issue + flavor combination, tracks overall status |
| `tasks` | `task_id` | Individual step executions within a mission |
| `runs` | `run_id` | Execution attempts of a task |

### State Machines

**Mission:** `pending → running → completed | failed`

**Task:** `queued → running → completed | failed`

**Run:** `queued → running → completed | failed`

---

## Technical Constraints

- **Control-Plane:** Rust, Axum, SQLite (rusqlite, WAL mode, foreign keys)
- **Console:** Astro SSR with Node adapter
- **Package manager:** Bun only — no npm, npx, yarn, or pnpm
- **GitHub API:** `gh` CLI or `GITHUB_TOKEN`
- **Deployment:** Single machine, single process

### Architecture Principles

- **Clean REST API:** Use resource-oriented routes with proper HTTP methods. Actions that mutate state use `POST` on a sub-resource (e.g. `POST /v1/repos/{id}/issues/refresh`), not query parameters. `GET` endpoints are always safe/idempotent.
- **Modular design:** Each domain (repos, issues, workflows, missions) follows the same layered structure end-to-end:
  - `models/{domain}.rs` — data structs with serde derives
  - `db/{domain}.rs` — SQLite query helpers (CRUD, no business logic)
  - `handlers/{domain}.rs` — HTTP handlers (orchestration, calls db + external services)
  - `lib/types.ts` — TypeScript interfaces mirroring backend models
  - `lib/api-client.ts` — typed fetch wrappers per endpoint
  - `components/{Domain}Card.astro` — display component
  - `pages/{domain}/index.astro` + `pages/{domain}/[id].astro` — SSR pages with path-based routing
- **Event-driven cache:** Caches are populated on first access and refreshed only when the user explicitly requests it. No TTLs or background polling.
- **Path-based routing:** Console pages use dynamic route segments (`/issues/[repo_id]`) instead of query parameters. Index pages redirect to a sensible default.
