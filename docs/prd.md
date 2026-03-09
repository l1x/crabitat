# Crabitat — Product Requirements Document

**Version:** 0.3.2
**Date:** 2026-03-07

---

## Overview

Crabitat is an orchestration platform for autonomous coding agents. It manages workflows that turn GitHub issues into executable missions. The system is designed for a **distributed agent farm** model where specialized workers ("Crabs") own the execution environment.

1.  **Control-Plane** — Rust backend serving the REST API, managing the SQLite database, and defining mission "Intent."
2.  **Crab Worker** — Git-aware Rust binary that handles the execution lifecycle (Clone, Burrow, Execute, Sync).
3.  **Console** — Web UI for operators to manage repos, issues, workflows, and missions.

---

## Core Concepts

1.  **Repo** is onboarded — requiring a `repo_url` (SSH). A `local_path` is optional for local-first development.
2.  **Issues** are loaded from GitHub.
3.  **Workflows** are global templates defined in TOML files.
4.  **Flavors** allow customizing workflows with specific tech-stack prompts.
5.  **Mission** = Issue + Workflow (+ Flavor) → produces **tasks**. Each mission defines a deterministic **branch** (`mission/issue-{number}`).
6.  **Burrow** — A `git worktree` created by the Crab worker to isolate a mission's execution.

---

## FR-1: Repository Management

- **FR-1.1**: Onboard a repo by providing `owner`, `name`, and `repo_url`.
- **FR-1.2**: Optional `local_path` for operators working on the same machine as the Control-Plane.
- **FR-1.3**: Automatic resolution of SSH keys via `ssh-agent` or cloud keystores (AWS Secrets Manager).

---

## FR-4: Missions and Prompt Assembly

### FR-4.1: Mission Intent
- Creating a mission defines the **target branch** and expands the workflow into persistent tasks.
- The Control-Plane does **not** modify the filesystem; it only records the intent.

### FR-4.2: Hybrid Prompt Assembly
Prompts are assembled in two stages to separate **intent** from **physicality**:
1.  **Static Assembly (Control-Plane):** Combines Base, Flavor, and Issue layers. Resolves `{{mission}}` and `{{context}}`. This defines the **intent** of the task.
2.  **Late-Binding Resolution (Crab):** Resolves environment-specific variables like `{{worktree_path}}` once the physical burrow has been created on the worker's filesystem.

---

## FR-5: Environment Lifecycle (The Crab)

The Crab worker is responsible for the physical environment:

1.  **Isolation:** Crabs create mission-specific burrows in a `burrows/` subdirectory (excluded via `.gitignore`).
2.  **Synchronization:** Crabs perform a `git fetch origin` before burrowing to ensure they have the latest state.
3.  **Cloning:** If no `local_path` is provided, the Crab clones the repo into a local cache.
4.  **Execution:** The Crab spawns the agent inside the burrow and captures all `stdout/stderr` output.
5.  **Performance Tracking:** Each run records **execution duration** (ms) and **token usage** (if reported by the agent).
6.  **Harvesting:** Upon success, the Crab `git push`es the burrow's branch back to the origin.
7.  **Traceability:** Data is never hard-deleted. Repos and Flavors use **soft-deletion** (`deleted_at`) to ensure that historical missions, tasks, and runs remain accessible for auditing even if their parent resources are removed from the active UI.
8.  **Cleanup:** (TBD) Burrows accumulate in the cache. A future requirement will involve pruning completed burrows to save disk space.

---

## Data Model

```
FileSystem (.agent-prompts/)
  └── workflows/*.toml (Read-only source for Workflows)
  └── prompts/**/*.md (Source for Base and Flavor layers)

Database (SQLite)
  ├── repos (repo_id, owner, name, repo_url, local_path?, deleted_at?)
  ├── settings (key, value)
  ├── environment_paths (environment, resource_type, resource_name, path)
  ├── workflow_flavors (flavor_id, workflow_name, name, prompt_paths_json, deleted_at?)
  ├── github_issues_cache (repo_id, number, title, body, labels, state)
  ├── missions (mission_id, repo_id, issue_number, workflow_name, flavor_id, branch, status)
  ├── tasks (task_id, mission_id, step_id, assembled_prompt, status)
  └── runs (run_id, task_id, status, logs, summary, duration_ms, tokens_used)
```

---

## Technical Constraints

- **Control-Plane:** Rust, Axum, SQLite (WAL mode).
- **Crab Worker:** Rust, Git CLI dependency.
- **Console:** Astro (SSR), Bun.
- **Isolation:** `git worktree` for branch-based isolation.
- **Security:** SSH-key based authentication for remote Git operations. Provisioning via instance profiles or local agents.
